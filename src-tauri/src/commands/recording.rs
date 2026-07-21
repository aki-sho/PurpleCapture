use super::ensure_main;
use crate::{
    models::{RecordingRequest, RecordingStatus},
    recording::RecordingManager,
    settings::SettingsStore,
    share::{SHARE_SOURCE_ID, ShareManager},
    sources,
};
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, State};

#[tauri::command]
pub async fn start_recording(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    request: RecordingRequest,
    recording: State<'_, Arc<RecordingManager>>,
    share: State<'_, Arc<ShareManager>>,
    settings: State<'_, Arc<SettingsStore>>,
) -> Result<RecordingStatus, String> {
    ensure_main(&webview)?;
    let is_share = request.source.kind == "browser";
    if is_share && request.source.id != SHARE_SOURCE_ID {
        return Err("共有タブIDが不正です。".into());
    }
    if is_share {
        share.set_recording_mode(&app, true);
    }
    let source = if is_share {
        match share.resolve_for_recording(&app) {
            Ok(source) => source,
            Err(cause) => {
                share.set_recording_mode(&app, false);
                return Err(format!("{cause:#}"));
            }
        }
    } else {
        sources::resolve_source(&request.source).map_err(|cause| format!("{cause:#}"))?
    };
    let save_directory = PathBuf::from(settings.get().save_directory);
    let recording = recording.inner().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        recording.start(request, source, save_directory)
    })
    .await
    .map_err(|cause| cause.to_string())?;
    let status = match result {
        Ok(status) => status,
        Err(cause) => {
            if is_share {
                share.set_recording_mode(&app, false);
            }
            return Err(format!("{cause:#}"));
        }
    };
    let _ = app.emit("recording-state", &status);
    Ok(status)
}

#[tauri::command]
pub fn pause_recording(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    recording: State<'_, Arc<RecordingManager>>,
) -> Result<RecordingStatus, String> {
    ensure_main(&webview)?;
    let status = recording.pause().map_err(|cause| format!("{cause:#}"))?;
    let _ = app.emit("recording-state", &status);
    Ok(status)
}

#[tauri::command]
pub fn resume_recording(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    recording: State<'_, Arc<RecordingManager>>,
) -> Result<RecordingStatus, String> {
    ensure_main(&webview)?;
    let status = recording.resume().map_err(|cause| format!("{cause:#}"))?;
    let _ = app.emit("recording-state", &status);
    Ok(status)
}

#[tauri::command]
pub async fn stop_recording(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    recording: State<'_, Arc<RecordingManager>>,
    share: State<'_, Arc<ShareManager>>,
) -> Result<RecordingStatus, String> {
    ensure_main(&webview)?;
    let recording = recording.inner().clone();
    let result = tauri::async_runtime::spawn_blocking(move || recording.stop())
        .await
        .map_err(|cause| cause.to_string())?;
    share.set_recording_mode(&app, false);
    let status = result.map_err(|cause| format!("{cause:#}"))?;
    let _ = app.emit("recording-state", &status);
    Ok(status)
}

#[tauri::command]
pub fn recording_status(
    webview: tauri::Webview,
    recording: State<'_, Arc<RecordingManager>>,
) -> Result<RecordingStatus, String> {
    ensure_main(&webview)?;
    Ok(recording.status())
}
