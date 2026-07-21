use super::{ensure_main, ensure_share};
use crate::{
    models::{ShareBrowserOption, ShareReport, ShareSessionInfo, ShareState},
    recording::RecordingManager,
    share::{SHARE_SOURCE_ID, ShareManager},
};
use std::sync::Arc;
use tauri::{Emitter, State};

#[tauri::command]
pub async fn share_open(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    share: State<'_, Arc<ShareManager>>,
) -> Result<ShareState, String> {
    ensure_main(&webview)?;
    share.open(&app).map_err(|cause| format!("{cause:#}"))
}

#[tauri::command]
pub fn share_status(
    webview: tauri::Webview,
    share: State<'_, Arc<ShareManager>>,
) -> Result<ShareState, String> {
    ensure_main(&webview)?;
    Ok(share.status())
}

#[tauri::command]
pub fn share_session_info(
    webview: tauri::Webview,
    share: State<'_, Arc<ShareManager>>,
) -> Result<ShareSessionInfo, String> {
    ensure_share(&webview)?;
    share.session_info().map_err(|cause| format!("{cause:#}"))
}

#[tauri::command]
pub fn share_launch_browser(
    webview: tauri::Webview,
    browser: String,
    share: State<'_, Arc<ShareManager>>,
) -> Result<ShareSessionInfo, String> {
    ensure_share(&webview)?;
    share
        .launch_browser(&browser)
        .map_err(|cause| format!("{cause:#}"))
}

#[tauri::command]
pub fn share_browser_options(
    webview: tauri::Webview,
    share: State<'_, Arc<ShareManager>>,
) -> Result<Vec<ShareBrowserOption>, String> {
    ensure_share(&webview)?;
    Ok(share.browser_options())
}

#[tauri::command]
pub fn share_stop(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    share: State<'_, Arc<ShareManager>>,
) -> Result<(), String> {
    ensure_main(&webview)?;
    share
        .stop_stream(&app)
        .map_err(|cause| format!("{cause:#}"))
}

#[tauri::command]
pub async fn share_report_state(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    report: ShareReport,
    share: State<'_, Arc<ShareManager>>,
    recording: State<'_, Arc<RecordingManager>>,
) -> Result<ShareState, String> {
    ensure_share(&webview)?;
    let was_active = share.status().active;
    let state = share
        .report(&app, report)
        .map_err(|cause| format!("{cause:#}"))?;
    if was_active && !state.active {
        let recording = recording.inner().clone();
        let app_handle = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Ok(Some(status)) = recording.stop_if_source(
                SHARE_SOURCE_ID,
                "タブ共有が停止されたため、安全に録画を停止しました。",
            ) {
                let _ = app_handle.emit("recording-state", &status);
            }
        })
        .await
        .map_err(|cause| cause.to_string())?;
    }
    Ok(state)
}

#[tauri::command]
pub fn share_focus_main(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    share: State<'_, Arc<ShareManager>>,
) -> Result<(), String> {
    ensure_share(&webview)?;
    share.focus_main(&app).map_err(|cause| format!("{cause:#}"))
}
