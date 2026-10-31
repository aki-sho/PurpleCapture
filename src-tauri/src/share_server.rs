use anyhow::{Context, Result, bail};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use uuid::Uuid;

const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const HEADER_TIMEOUT: Duration = Duration::from_secs(1);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const EMPTY_CONNECTION_ERROR: &str = "共有用ローカル通信がリクエストなしで終了しました。";
const SENDER_HTML: &str = include_str!("../../src/share-sender.html");
const SENDER_JS: &str = include_str!("../../src/share-sender.js");
const SENDER_CSS: &str = include_str!("../../src/share-sender.css");

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SignalSnapshot {
    #[serde(skip)]
    revoked: bool,
    offer: Option<String>,
    answer: Option<String>,
    active: bool,
    title: String,
    display_surface: String,
    width: u32,
    height: u32,
    revision: u64,
}

impl SignalSnapshot {
    fn revoke(&mut self) {
        self.revoked = true;
        self.active = false;
        self.offer = None;
        self.answer = None;
        self.title.clear();
        self.display_surface.clear();
        self.width = 0;
        self.height = 0;
        self.revision = self.revision.saturating_add(1);
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfferRequest {
    sdp: String,
    title: String,
    display_surface: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize)]
struct AnswerRequest {
    sdp: String,
}

pub struct ShareServer {
    address: SocketAddr,
    token: String,
    signal: Arc<Mutex<SignalSnapshot>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ShareServer {
    pub fn start() -> Result<Self> {
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).context("共有用ローカル通信を開始できません。")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let token = Uuid::new_v4().simple().to_string();
        let signal = Arc::new(Mutex::new(SignalSnapshot::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_signal = signal.clone();
        let worker_stop = stop.clone();
        let worker_token = token.clone();
        let thread = thread::Builder::new()
            .name("purplecapture-share-server".into())
            .spawn(move || {
                run_server(listener, &worker_token, worker_signal, worker_stop);
            })
            .context("共有用ローカル通信スレッドを開始できません。")?;
        Ok(Self {
            address,
            token,
            signal,
            stop,
            thread: Some(thread),
        })
    }

    pub fn sender_url(&self) -> String {
        format!("http://{}/share/{}", self.address, self.token)
    }

    pub fn api_base(&self) -> String {
        format!("http://{}/api", self.address)
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn stop_session(&self) {
        self.signal.lock().revoke();
    }

    pub fn is_revoked(&self) -> bool {
        self.signal.lock().revoked
    }

    pub fn shutdown(&mut self) {
        self.stop_session();
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ShareServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_server(
    listener: TcpListener,
    token: &str,
    signal: Arc<Mutex<SignalSnapshot>>,
    stop: Arc<AtomicBool>,
) {
    const MAX_CONNECTIONS: usize = 32;
    let active_connections = Arc::new(AtomicUsize::new(0));
    let mut workers: Vec<JoinHandle<()>> = Vec::new();

    while !stop.load(Ordering::Acquire) {
        workers.retain(|worker| !worker.is_finished());
        match listener.accept() {
            Ok((stream, _)) => {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                if active_connections.load(Ordering::Acquire) >= MAX_CONNECTIONS {
                    drop(stream);
                    continue;
                }

                active_connections.fetch_add(1, Ordering::AcqRel);
                let connection_count = active_connections.clone();
                let connection_signal = signal.clone();
                let connection_token = token.to_owned();
                let spawn_result = thread::Builder::new()
                    .name("purplecapture-share-request".into())
                    .spawn(move || {
                        if let Err(cause) =
                            handle_connection(stream, &connection_token, &connection_signal)
                            && !is_idle_connection_error(&cause)
                        {
                            eprintln!("Purple Capture share server request failed: {cause:#}");
                        }
                        connection_count.fetch_sub(1, Ordering::AcqRel);
                    });
                match spawn_result {
                    Ok(worker) => workers.push(worker),
                    Err(cause) => {
                        active_connections.fetch_sub(1, Ordering::AcqRel);
                        eprintln!("Purple Capture share server worker failed: {cause}");
                    }
                }
            }
            Err(cause) if cause.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break,
        }
    }
    for worker in workers {
        let _ = worker.join();
    }
}

fn is_idle_connection_error(cause: &anyhow::Error) -> bool {
    cause.to_string() == EMPTY_CONNECTION_ERROR
        || cause.chain().any(|entry| {
            entry.downcast_ref::<std::io::Error>().is_some_and(|error| {
                matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::BrokenPipe
                )
            })
        })
}

fn handle_connection(
    mut stream: TcpStream,
    token: &str,
    signal: &Arc<Mutex<SignalSnapshot>>,
) -> Result<()> {
    // Windowsでは、非ブロッキングのTcpListenerからacceptしたソケットも
    // 非ブロッキングになる場合がある。HTTP処理はタイムアウト付きの
    // ブロッキング読み取りを前提とするため、接続ごとに明示的に戻す。
    stream
        .set_nonblocking(false)
        .context("共有用ローカル通信を読み取りモードへ切り替えられません。")?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let started = Instant::now();
    let mut request = read_headers(&mut stream, started + HEADER_TIMEOUT)?;

    let share_path = format!("/share/{token}");
    let session_path = format!("/api/session/{token}");
    let offer_path = format!("/api/offer/{token}");
    let answer_path = format!("/api/answer/{token}");
    let stop_path = format!("/api/stop/{token}");

    let public_asset =
        request.method == "GET" && matches!(request.path.as_str(), "/sender.js" | "/sender.css");
    let valid_route = match request.method.as_str() {
        "GET" => request.path == share_path || request.path == session_path || public_asset,
        "POST" => {
            request.path == offer_path || request.path == answer_path || request.path == stop_path
        }
        "OPTIONS" => [
            session_path.as_str(),
            offer_path.as_str(),
            answer_path.as_str(),
            stop_path.as_str(),
        ]
        .contains(&request.path.as_str()),
        _ => false,
    };
    // Authenticate the route before waiting for any request body.
    if !valid_route {
        return send_response(&mut stream, 404, "text/plain", b"Not Found");
    }
    if !public_asset && signal.lock().revoked {
        return send_expired(&mut stream);
    }
    if request.method == "OPTIONS" {
        return send_response(&mut stream, 204, "text/plain; charset=utf-8", b"");
    }
    read_body(&mut stream, &mut request, started + REQUEST_TIMEOUT)?;

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", path) if path == share_path => send_response(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            SENDER_HTML.as_bytes(),
        ),
        ("GET", "/sender.js") => send_response(
            &mut stream,
            200,
            "text/javascript; charset=utf-8",
            SENDER_JS.as_bytes(),
        ),
        ("GET", "/sender.css") => send_response(
            &mut stream,
            200,
            "text/css; charset=utf-8",
            SENDER_CSS.as_bytes(),
        ),
        ("GET", path) if path == session_path => {
            let state = signal.lock();
            if state.revoked {
                drop(state);
                return send_expired(&mut stream);
            }
            let body = serde_json::to_vec(&*state)?;
            drop(state);
            send_response(&mut stream, 200, "application/json", &body)
        }
        ("POST", path) if path == offer_path => {
            let offer: OfferRequest =
                serde_json::from_slice(&request.body).context("共有情報が不正です。")?;
            validate_offer(&offer)?;
            let mut state = signal.lock();
            if state.revoked {
                drop(state);
                return send_expired(&mut stream);
            }
            state.offer = Some(offer.sdp);
            state.answer = None;
            state.active = true;
            state.title = if offer.title.trim().is_empty() {
                "共有タブ".into()
            } else {
                offer.title
            };
            state.display_surface = offer.display_surface;
            state.width = offer.width;
            state.height = offer.height;
            state.revision = state.revision.saturating_add(1);
            drop(state);
            send_json_ok(&mut stream)
        }
        ("POST", path) if path == answer_path => {
            let answer: AnswerRequest =
                serde_json::from_slice(&request.body).context("応答情報が不正です。")?;
            if answer.sdp.len() > 1_000_000 || !answer.sdp.starts_with("v=0") {
                bail!("応答SDPが不正です。");
            }
            let mut state = signal.lock();
            if state.revoked {
                drop(state);
                return send_expired(&mut stream);
            }
            state.answer = Some(answer.sdp);
            drop(state);
            send_json_ok(&mut stream)
        }
        ("POST", path) if path == stop_path => {
            let mut state = signal.lock();
            if state.revoked {
                drop(state);
                return send_expired(&mut stream);
            }
            state.revoke();
            drop(state);
            send_json_ok(&mut stream)
        }
        _ => send_response(
            &mut stream,
            404,
            "text/plain; charset=utf-8",
            "Not Found".as_bytes(),
        ),
    }
}

fn validate_offer(offer: &OfferRequest) -> Result<()> {
    if offer.sdp.len() > 1_000_000 || !offer.sdp.starts_with("v=0") {
        bail!("共有SDPが不正です。");
    }
    if offer.title.len() > 512
        || !matches!(
            offer.display_surface.as_str(),
            "browser" | "window" | "monitor" | "unknown"
        )
        || !(1..=16_384).contains(&offer.width)
        || !(1..=16_384).contains(&offer.height)
    {
        bail!("共有メタデータが不正です。");
    }
    Ok(())
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
    content_length: usize,
}

fn read_before(stream: &mut TcpStream, buffer: &mut [u8], deadline: Instant) -> Result<usize> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|value| !value.is_zero())
        .context("共有用HTTPリクエストの制限時間を超えました。")?;
    stream.set_read_timeout(Some(remaining))?;
    Ok(stream.read(buffer)?)
}

fn read_headers(stream: &mut TcpStream, deadline: Instant) -> Result<HttpRequest> {
    let mut data = Vec::new();
    let mut buffer = [0_u8; 8192];
    let header_end;
    loop {
        let count = read_before(stream, &mut buffer, deadline)?;
        if count == 0 {
            if data.is_empty() {
                bail!(EMPTY_CONNECTION_ERROR);
            }
            bail!("HTTPリクエストが途中で終了しました。");
        }
        data.extend_from_slice(&buffer[..count]);
        if let Some(index) = find_header_end(&data) {
            if index + 4 > MAX_HEADER_BYTES {
                bail!("HTTPヘッダーが大きすぎます。");
            }
            header_end = index;
            break;
        }
        if data.len() >= MAX_HEADER_BYTES {
            bail!("HTTPヘッダーが大きすぎます。");
        }
    }
    let headers = std::str::from_utf8(&data[..header_end])?;
    let mut lines = headers.lines();
    let mut request_line = lines
        .next()
        .context("HTTPリクエスト行がありません。")?
        .split_whitespace();
    let method = request_line
        .next()
        .context("HTTPメソッドがありません.")?
        .to_string();
    let path = request_line
        .next()
        .context("HTTPパスがありません。")?
        .to_string();
    if request_line.next() != Some("HTTP/1.1")
        || request_line.next().is_some()
        || !matches!(method.as_str(), "GET" | "POST" | "OPTIONS")
        || !path.starts_with('/')
    {
        bail!("HTTPリクエストが不正です。");
    }
    let mut length = None;
    for line in lines {
        let (name, value) = line.split_once(':').context("HTTPヘッダーが不正です。")?;
        if name.eq_ignore_ascii_case("transfer-encoding") {
            bail!("転送エンコードは利用できません。");
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some()
                || value.trim().is_empty()
                || !value.trim().bytes().all(|byte| byte.is_ascii_digit())
            {
                bail!("HTTP本文長が不正です。");
            }
            length = Some(value.trim().parse::<usize>()?);
        }
    }
    let content_length = length.unwrap_or(0);
    if content_length > MAX_REQUEST_BYTES {
        bail!("HTTP本文が大きすぎます。");
    }
    if method != "POST" && content_length != 0 {
        bail!("このHTTPメソッドでは本文を利用できません。");
    }
    let body_start = header_end + 4;
    Ok(HttpRequest {
        method,
        path: path.split('?').next().unwrap_or(&path).into(),
        body: data[body_start..]
            .iter()
            .take(content_length)
            .copied()
            .collect(),
        content_length,
    })
}

fn read_body(stream: &mut TcpStream, request: &mut HttpRequest, deadline: Instant) -> Result<()> {
    let mut buffer = [0_u8; 8192];
    while request.body.len() < request.content_length {
        let remaining = (request.content_length - request.body.len()).min(buffer.len());
        let count = read_before(stream, &mut buffer[..remaining], deadline)?;
        if count == 0 {
            bail!("HTTP本文が途中で終了しました。");
        }
        request.body.extend_from_slice(&buffer[..count]);
    }
    Ok(())
}

fn find_header_end(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|value| value == b"\r\n\r\n")
}

fn send_json_ok(stream: &mut TcpStream) -> Result<()> {
    send_response(stream, 200, "application/json", br#"{"ok":true}"#)
}

fn send_expired(stream: &mut TcpStream) -> Result<()> {
    send_response(stream, 410, "text/plain", b"Share session expired")
}

fn send_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        410 => "Gone",
        _ => "Error",
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; media-src blob:; img-src 'none'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Shutdown;

    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    #[test]
    fn unauthenticated_body_is_rejected_before_it_is_received() {
        let mut server = ShareServer::start().unwrap();
        let mut stream = TcpStream::connect(server.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(750)))
            .unwrap();
        stream.write_all(b"POST /api/offer/invalid HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1000000\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 404"));
        server.shutdown();
    }

    #[test]
    fn trickling_headers_and_body_cannot_extend_absolute_deadlines() {
        for body in [false, true] {
            let (mut client, mut stream) = pair();
            let writer = thread::spawn(move || {
                if body {
                    client
                        .write_all(b"POST /api/test HTTP/1.1\r\nContent-Length: 10000\r\n\r\n")
                        .unwrap();
                }
                for _ in 0..100 {
                    if client.write_all(b"x").is_err() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
            });
            let started = Instant::now();
            let deadline = started + Duration::from_millis(160);
            if body {
                let mut request = read_headers(&mut stream, deadline).unwrap();
                assert!(read_body(&mut stream, &mut request, deadline).is_err());
            } else {
                assert!(read_headers(&mut stream, deadline).is_err());
            }
            assert!(started.elapsed() < Duration::from_millis(800));
            drop(stream);
            writer.join().unwrap();
        }
    }

    #[test]
    fn oversized_or_ambiguous_headers_are_rejected() {
        let invalid = [
            format!(
                "GET / HTTP/1.1\r\nX-Large: {}\r\n\r\n",
                "x".repeat(MAX_HEADER_BYTES)
            ),
            "POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\n".into(),
            "POST / HTTP/1.1\r\nContent-Length: invalid\r\n\r\n".into(),
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n".into(),
        ];
        for value in invalid {
            let (mut client, mut stream) = pair();
            let writer = thread::spawn(move || {
                let _ = client.write_all(value.as_bytes());
            });
            assert!(read_headers(&mut stream, Instant::now() + Duration::from_secs(1)).is_err());
            drop(stream);
            writer.join().unwrap();
        }
    }

    #[test]
    fn stopping_revokes_token_and_rejects_an_already_authenticated_body() {
        let mut server = ShareServer::start().unwrap();
        let body =
            r#"{"sdp":"v=0","title":"test","displaySurface":"browser","width":1,"height":1}"#;
        let mut pending = TcpStream::connect(server.address).unwrap();
        pending
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        write!(
            pending,
            "POST /api/offer/{} HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            server.token,
            body.len()
        )
        .unwrap();
        thread::sleep(Duration::from_millis(50));
        server.stop_session();
        pending.write_all(body.as_bytes()).unwrap();
        let mut response = String::new();
        pending.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 410"));
        for (method, path, data) in [
            ("GET", format!("/share/{}", server.token), ""),
            ("GET", format!("/api/session/{}", server.token), ""),
            ("POST", format!("/api/offer/{}", server.token), body),
            (
                "POST",
                format!("/api/answer/{}", server.token),
                r#"{"sdp":"v=0"}"#,
            ),
        ] {
            assert!(request(&server, method, &path, data).starts_with("HTTP/1.1 410"));
        }
        assert!(!server.signal.lock().active);
        server.shutdown();
    }

    fn request(server: &ShareServer, method: &str, path: &str, body: &str) -> String {
        let mut stream = TcpStream::connect(server.address).expect("connect share server");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set response timeout");
        let value = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            server.address,
            body.len()
        );
        stream.write_all(value.as_bytes()).expect("write request");
        stream.shutdown(Shutdown::Write).expect("finish request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        response
    }

    #[test]
    fn loopback_share_server_routes_are_token_scoped() {
        let mut server = ShareServer::start().expect("start share server");
        let page = request(&server, "GET", &format!("/share/{}", server.token), "");
        assert!(page.starts_with("HTTP/1.1 200"));
        assert!(page.contains("Purple Capture"));

        let denied = request(&server, "GET", "/share/wrong-token", "");
        assert!(denied.starts_with("HTTP/1.1 404"));

        let offer = serde_json::json!({
            "sdp": "v=0\r\ns=test",
            "title": "Test browser tab",
            "displaySurface": "browser",
            "width": 1920,
            "height": 1080
        })
        .to_string();
        let accepted = request(
            &server,
            "POST",
            &format!("/api/offer/{}", server.token),
            &offer,
        );
        assert!(accepted.starts_with("HTTP/1.1 200"));

        let session = request(
            &server,
            "GET",
            &format!("/api/session/{}", server.token),
            "",
        );
        assert!(session.contains("\"active\":true"));
        assert!(session.contains("Test browser tab"));

        let stopped = request(
            &server,
            "POST",
            &format!("/api/stop/{}", server.token),
            "{}",
        );
        assert!(stopped.starts_with("HTTP/1.1 200"));
        let expired = request(
            &server,
            "GET",
            &format!("/api/session/{}", server.token),
            "",
        );
        assert!(expired.starts_with("HTTP/1.1 410"));
        server.shutdown();
    }

    #[test]
    fn loopback_share_server_is_not_blocked_by_idle_browser_connection() {
        let mut server = ShareServer::start().expect("start share server");
        let _idle_connection =
            TcpStream::connect(server.address).expect("open speculative browser connection");
        thread::sleep(Duration::from_millis(50));

        let response = request(
            &server,
            "GET",
            &format!("/api/session/{}", server.token),
            "",
        );
        assert!(response.starts_with("HTTP/1.1 200"));

        server.shutdown();
    }

    #[test]
    fn loopback_share_server_handles_repeated_polling() {
        let mut server = ShareServer::start().expect("start share server");
        let session_path = format!("/api/session/{}", server.token);

        for attempt in 0..100 {
            let response = request(&server, "GET", &session_path, "");
            assert!(
                response.starts_with("HTTP/1.1 200"),
                "poll {attempt} failed: {response}"
            );
        }

        server.shutdown();
    }

    #[test]
    #[ignore = "外部ブラウザのgetDisplayMedia選択画面を確認する手動テスト"]
    fn manual_share_sender_page() {
        let mut server = ShareServer::start().expect("start share server");
        println!("SHARE_TEST_URL={}", server.sender_url());
        if let Ok(path) = std::env::var("PURPLECAPTURE_SHARE_TEST_URL_FILE") {
            std::fs::write(path, server.sender_url()).expect("write share test URL");
        }
        let seconds = std::env::var("PURPLECAPTURE_SHARE_TEST_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(300)
            .clamp(1, 1_800);
        for _ in 0..seconds {
            thread::sleep(Duration::from_secs(1));
        }
        server.shutdown();
    }
}
