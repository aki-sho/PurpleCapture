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
    time::Duration,
};
use uuid::Uuid;

const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
const EMPTY_CONNECTION_ERROR: &str = "共有用ローカル通信がリクエストなしで終了しました。";
const SENDER_HTML: &str = include_str!("../../src/share-sender.html");
const SENDER_JS: &str = include_str!("../../src/share-sender.js");
const SENDER_CSS: &str = include_str!("../../src/share-sender.css");

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SignalSnapshot {
    offer: Option<String>,
    answer: Option<String>,
    active: bool,
    title: String,
    display_surface: String,
    width: u32,
    height: u32,
    revision: u64,
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
        let mut signal = self.signal.lock();
        signal.active = false;
        signal.offer = None;
        signal.answer = None;
        signal.revision = signal.revision.saturating_add(1);
    }

    pub fn shutdown(&mut self) {
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

    while !stop.load(Ordering::Acquire) {
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
                if let Err(cause) = spawn_result {
                    active_connections.fetch_sub(1, Ordering::AcqRel);
                    eprintln!("Purple Capture share server worker failed: {cause}");
                }
            }
            Err(cause) if cause.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break,
        }
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
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let request = read_request(&mut stream)?;
    if request.method == "OPTIONS" {
        return send_response(&mut stream, 204, "text/plain; charset=utf-8", b"");
    }

    let share_path = format!("/share/{token}");
    let session_path = format!("/api/session/{token}");
    let offer_path = format!("/api/offer/{token}");
    let answer_path = format!("/api/answer/{token}");
    let stop_path = format!("/api/stop/{token}");

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
            let body = serde_json::to_vec(&*signal.lock())?;
            send_response(&mut stream, 200, "application/json", &body)
        }
        ("POST", path) if path == offer_path => {
            let offer: OfferRequest =
                serde_json::from_slice(&request.body).context("共有情報が不正です。")?;
            validate_offer(&offer)?;
            let mut state = signal.lock();
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
            send_json_ok(&mut stream)
        }
        ("POST", path) if path == answer_path => {
            let answer: AnswerRequest =
                serde_json::from_slice(&request.body).context("応答情報が不正です。")?;
            if answer.sdp.len() > 1_000_000 || !answer.sdp.starts_with("v=0") {
                bail!("応答SDPが不正です。");
            }
            signal.lock().answer = Some(answer.sdp);
            send_json_ok(&mut stream)
        }
        ("POST", path) if path == stop_path => {
            let mut state = signal.lock();
            state.active = false;
            state.offer = None;
            state.answer = None;
            state.revision = state.revision.saturating_add(1);
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
}

fn read_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    let mut data = Vec::new();
    let mut buffer = [0_u8; 8192];
    let header_end;
    loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            if data.is_empty() {
                bail!(EMPTY_CONNECTION_ERROR);
            }
            bail!("HTTPリクエストが途中で終了しました。");
        }
        data.extend_from_slice(&buffer[..count]);
        if data.len() > MAX_REQUEST_BYTES {
            bail!("HTTPリクエストが大きすぎます。");
        }
        if let Some(index) = find_header_end(&data) {
            header_end = index;
            break;
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
    if !matches!(method.as_str(), "GET" | "POST" | "OPTIONS") || !path.starts_with('/') {
        bail!("HTTPリクエストが不正です。");
    }
    let content_length = lines
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    if content_length > MAX_REQUEST_BYTES {
        bail!("HTTP本文が大きすぎます。");
    }
    let body_start = header_end + 4;
    while data.len().saturating_sub(body_start) < content_length {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            bail!("HTTP本文が途中で終了しました。");
        }
        data.extend_from_slice(&buffer[..count]);
        if data.len() > MAX_REQUEST_BYTES {
            bail!("HTTPリクエストが大きすぎます。");
        }
    }
    Ok(HttpRequest {
        method,
        path: path.split('?').next().unwrap_or(&path).into(),
        body: data[body_start..body_start + content_length].to_vec(),
    })
}

fn find_header_end(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|value| value == b"\r\n\r\n")
}

fn send_json_ok(stream: &mut TcpStream) -> Result<()> {
    send_response(stream, 200, "application/json", br#"{"ok":true}"#)
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
