use crate::{
    models::{CaptureSource, ShareBrowserOption, ShareReport, ShareSessionInfo, ShareState},
    portable_paths::PortablePaths,
    share_server::ShareServer,
    sources::ResolvedSource,
};
use anyhow::{Context, Result, bail};
use parking_lot::Mutex;
use std::{
    env,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use windows_capture::window::Window as CaptureWindow;

pub const SHARE_SOURCE_ID: &str = "share:preview";
pub const SHARE_WINDOW_LABEL: &str = "share-preview";

pub struct ShareManager {
    paths: Arc<PortablePaths>,
    state: Mutex<ShareState>,
    server: Mutex<Option<ShareServer>>,
}

impl ShareManager {
    pub fn new(paths: Arc<PortablePaths>) -> Self {
        Self {
            paths,
            state: Mutex::new(ShareState::default()),
            server: Mutex::new(None),
        }
    }

    pub fn status(&self) -> ShareState {
        self.state.lock().clone()
    }

    pub fn capture_sources(&self) -> Vec<CaptureSource> {
        let state = self.status();
        if !state.window_open || !state.active {
            return Vec::new();
        }
        let surface = match state.display_surface.as_str() {
            "browser" => "ブラウザタブ",
            "window" => "ウィンドウ",
            "monitor" => "モニター",
            _ => "共有画面",
        };
        vec![CaptureSource {
            id: SHARE_SOURCE_ID.into(),
            kind: "browser".into(),
            name: state.title,
            detail: format!("{surface}を共有中 · {} × {}", state.width, state.height),
        }]
    }

    pub fn open(&self, app: &AppHandle) -> Result<ShareState> {
        self.ensure_server()?;
        if let Some(window) = app.get_webview_window(SHARE_WINDOW_LABEL) {
            window.unminimize()?;
            window.show()?;
            window.set_minimizable(false)?;
            window.set_focus()?;
            self.state.lock().window_open = true;
            self.emit(app);
            return Ok(self.status());
        }

        let result = WebviewWindowBuilder::new(
            app,
            SHARE_WINDOW_LABEL,
            WebviewUrl::App("share.html".into()),
        )
        .title("Purple Capture - タブ共有")
        .inner_size(1280.0, 720.0)
        .min_inner_size(640.0, 360.0)
        .center()
        .resizable(true)
        .minimizable(false)
        .devtools(cfg!(debug_assertions))
        // メインWebViewと同じPortableDataを明示し、Tauriの既定AppData
        // フォルダ作成とWebView2環境の不一致を同時に防ぐ。
        .data_directory(self.paths.webview.clone())
        .build();
        if let Err(cause) = result {
            let detail = format!("Share preview creation failed: {cause}");
            self.paths.log("ERROR", &detail);
            self.shutdown_server();
            return Err(cause).context("タブ共有ウィンドウを作成できません。");
        }

        self.state.lock().window_open = true;
        self.emit(app);
        Ok(self.status())
    }

    pub fn session_info(&self) -> Result<ShareSessionInfo> {
        self.ensure_server()
    }

    pub fn browser_options(&self) -> Vec<ShareBrowserOption> {
        vec![
            ShareBrowserOption {
                id: "default".into(),
                name: "既定のブラウザ".into(),
                available: true,
            },
            ShareBrowserOption {
                id: "chrome".into(),
                name: "Google Chrome".into(),
                available: find_browser_executable("chrome").is_some(),
            },
            ShareBrowserOption {
                id: "edge".into(),
                name: "Microsoft Edge".into(),
                available: find_browser_executable("edge").is_some(),
            },
        ]
    }

    pub fn launch_browser(&self, browser: &str) -> Result<ShareSessionInfo> {
        let info = self.ensure_server()?;
        match browser {
            "default" => tauri_plugin_opener::open_url(&info.sender_url, None::<&str>),
            "chrome" | "edge" => {
                let executable = find_browser_executable(browser)
                    .with_context(|| format!("選択したブラウザ（{browser}）が見つかりません。"))?;
                tauri_plugin_opener::open_url(
                    &info.sender_url,
                    Some(executable.to_string_lossy().into_owned()),
                )
            }
            _ => bail!("共有先ブラウザの指定が不正です。"),
        }
        .context("外部ブラウザで共有ページを開けません。")?;
        Ok(info)
    }

    pub fn report(&self, app: &AppHandle, report: ShareReport) -> Result<ShareState> {
        report.validate()?;
        let mut state = self.state.lock();
        state.window_open = app.get_webview_window(SHARE_WINDOW_LABEL).is_some();
        state.active = report.active;
        if report.active {
            state.title = if report.title.trim().is_empty() {
                "共有タブ".into()
            } else {
                report.title
            };
            state.display_surface = report.display_surface;
            state.width = report.width;
            state.height = report.height;
            state.has_audio = report.has_audio;
        } else {
            state.title = "共有タブ".into();
            state.display_surface.clear();
            state.width = 0;
            state.height = 0;
            state.has_audio = false;
        }
        let result = state.clone();
        drop(state);

        if let Some(window) = app.get_webview_window(SHARE_WINDOW_LABEL) {
            let title = if result.active {
                format!("Purple Capture - {}", result.title)
            } else {
                "Purple Capture - タブ共有".into()
            };
            let _ = window.set_title(&title);
        }
        self.emit(app);
        Ok(result)
    }

    pub fn stop_stream(&self, app: &AppHandle) -> Result<()> {
        if let Some(server) = self.server.lock().as_ref() {
            server.stop_session();
        }
        let window = app
            .get_webview_window(SHARE_WINDOW_LABEL)
            .context("タブ共有ウィンドウが開いていません。")?;
        window.eval("window.purpleCaptureShare?.stopFromHost?.()")?;
        Ok(())
    }

    pub fn focus_main(&self, app: &AppHandle) -> Result<()> {
        let window = app
            .get_webview_window("main")
            .context("メインウィンドウが見つかりません。")?;
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
        let _ = app.emit("share-use-requested", ());
        Ok(())
    }

    pub fn set_recording_mode(&self, app: &AppHandle, active: bool) {
        if let Some(window) = app.get_webview_window(SHARE_WINDOW_LABEL) {
            let script = format!(
                "window.purpleCaptureShare?.setRecordingMode?.({})",
                if active { "true" } else { "false" }
            );
            let _ = window.eval(&script);
        }
    }

    pub fn resolve_for_recording(&self, app: &AppHandle) -> Result<ResolvedSource> {
        let state = self.status();
        if !state.window_open || !state.active {
            bail!("先にブラウザ画面から録画するタブを共有してください。");
        }
        let window = app
            .get_webview_window(SHARE_WINDOW_LABEL)
            .context("タブ共有ウィンドウが見つかりません。")?;
        window.unminimize()?;
        window.show()?;
        window.set_minimizable(false)?;
        // 録画UIを隠してCanvasだけを合成する1フレーム分を待つ。
        thread::sleep(Duration::from_millis(120));
        let hwnd = window.hwnd()?.0 as usize;
        let size = window.inner_size()?;
        Ok(ResolvedSource::Window {
            window: CaptureWindow::from_raw_hwnd(hwnd as *mut std::ffi::c_void),
            width: size.width.max(1),
            height: size.height.max(1),
        })
    }

    pub fn mark_window_closed(&self, app: &AppHandle) {
        *self.state.lock() = ShareState::default();
        self.shutdown_server();
        self.emit(app);
    }

    pub fn close_all(&self, app: &AppHandle) {
        if let Some(server) = self.server.lock().as_ref() {
            server.stop_session();
        }
        if let Some(window) = app.get_webview_window(SHARE_WINDOW_LABEL) {
            let _ = window.close();
        }
        *self.state.lock() = ShareState::default();
        self.shutdown_server();
    }

    fn emit(&self, app: &AppHandle) {
        let _ = app.emit("share-state-changed", self.status());
    }

    fn ensure_server(&self) -> Result<ShareSessionInfo> {
        let mut server = self.server.lock();
        if server.is_none() {
            *server = Some(ShareServer::start()?);
        }
        let server = server
            .as_ref()
            .context("共有セッションを開始できません。")?;
        Ok(ShareSessionInfo {
            sender_url: server.sender_url(),
            api_base: server.api_base(),
            token: server.token().into(),
        })
    }

    fn shutdown_server(&self) {
        if let Some(mut server) = self.server.lock().take() {
            server.shutdown();
        }
    }
}

fn find_browser_executable(browser: &str) -> Option<PathBuf> {
    let relative_paths: &[(&str, &str)] = match browser {
        "chrome" => &[
            ("ProgramFiles", r"Google\Chrome\Application\chrome.exe"),
            ("ProgramFiles(x86)", r"Google\Chrome\Application\chrome.exe"),
            ("LOCALAPPDATA", r"Google\Chrome\Application\chrome.exe"),
        ],
        "edge" => &[
            ("ProgramFiles", r"Microsoft\Edge\Application\msedge.exe"),
            (
                "ProgramFiles(x86)",
                r"Microsoft\Edge\Application\msedge.exe",
            ),
            ("LOCALAPPDATA", r"Microsoft\Edge\Application\msedge.exe"),
        ],
        _ => return None,
    };

    relative_paths.iter().find_map(|(variable, relative)| {
        let root = env::var_os(variable)?;
        let candidate = Path::new(&root).join(relative);
        candidate.is_file().then_some(candidate)
    })
}
