use super::ensure_main;
use crate::{
    history::HistoryStore,
    models::{AppSettings, BootstrapData, HistoryEntry},
    recording::RecordingManager,
    settings::SettingsStore,
    share::ShareManager,
    sources,
};
use std::{path::PathBuf, sync::Arc};
use tauri::State;
use tauri_plugin_dialog::{DialogExt, FilePath};

#[tauri::command]
pub fn bootstrap(
    webview: tauri::Webview,
    settings: State<'_, Arc<SettingsStore>>,
    recording: State<'_, Arc<RecordingManager>>,
    share: State<'_, Arc<ShareManager>>,
) -> Result<BootstrapData, String> {
    ensure_main(&webview)?;
    Ok(BootstrapData {
        settings: settings.get(),
        recording: recording.status(),
        share: share.status(),
    })
}

#[tauri::command]
pub async fn list_capture_sources(
    webview: tauri::Webview,
    kind: String,
    share: State<'_, Arc<ShareManager>>,
) -> Result<Vec<crate::models::CaptureSource>, String> {
    ensure_main(&webview)?;
    match kind.as_str() {
        "monitor" => tauri::async_runtime::spawn_blocking(sources::list_monitors)
            .await
            .map_err(|cause| cause.to_string())?
            .map_err(|cause| format!("{cause:#}")),
        "window" => tauri::async_runtime::spawn_blocking(sources::list_windows)
            .await
            .map_err(|cause| cause.to_string())?
            .map_err(|cause| format!("{cause:#}")),
        "browser" => Ok(share.capture_sources()),
        _ => Err("録画対象の種類が不正です。".into()),
    }
}

#[tauri::command]
pub async fn list_microphones(
    webview: tauri::Webview,
) -> Result<Vec<crate::models::AudioDevice>, String> {
    ensure_main(&webview)?;
    tauri::async_runtime::spawn_blocking(crate::recording::list_microphones)
        .await
        .map_err(|cause| cause.to_string())?
        .map_err(|cause| format!("{cause:#}"))
}

#[tauri::command]
pub fn recording_history(
    webview: tauri::Webview,
    history: State<'_, Arc<HistoryStore>>,
) -> Result<Vec<HistoryEntry>, String> {
    ensure_main(&webview)?;
    Ok(history.list())
}

#[tauri::command]
pub fn open_recording(
    webview: tauri::Webview,
    path: String,
    history: State<'_, Arc<HistoryStore>>,
) -> Result<(), String> {
    ensure_main(&webview)?;
    let path = PathBuf::from(path);
    if !path.is_file() || !history.contains_path(&path) {
        return Err("録画履歴にないファイルは開けません。".into());
    }
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|cause| cause.to_string())
}

#[tauri::command]
pub fn open_recordings_folder(
    webview: tauri::Webview,
    settings: State<'_, Arc<SettingsStore>>,
) -> Result<(), String> {
    ensure_main(&webview)?;
    let path = PathBuf::from(settings.get().save_directory);
    if !path.is_dir() {
        return Err("保存フォルダが見つかりません。".into());
    }
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|cause| cause.to_string())
}

#[tauri::command]
pub async fn choose_save_folder(
    webview: tauri::Webview,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    ensure_main(&webview)?;
    let selected = app.dialog().file().blocking_pick_folder();
    Ok(match selected {
        Some(FilePath::Path(path)) => Some(path.to_string_lossy().into_owned()),
        Some(_) => return Err("ローカルフォルダだけを選択できます。".into()),
        None => None,
    })
}

#[tauri::command]
pub fn save_settings(
    webview: tauri::Webview,
    settings: AppSettings,
    store: State<'_, Arc<SettingsStore>>,
) -> Result<AppSettings, String> {
    ensure_main(&webview)?;
    store.save(settings).map_err(|cause| format!("{cause:#}"))
}
