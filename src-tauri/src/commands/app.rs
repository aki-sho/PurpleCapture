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
pub fn open_license_documents(
    webview: tauri::Webview,
    paths: State<'_, Arc<crate::portable_paths::PortablePaths>>,
) -> Result<(), String> {
    ensure_main(&webview)?;
    let directory = paths.root.join("licenses");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    for (name, contents) in [
        ("LICENSE.txt", include_str!("../../../LICENSE")),
        (
            "THIRD_PARTY_NOTICES.md",
            include_str!("../../../THIRD_PARTY_NOTICES.md"),
        ),
        (
            "THIRD_PARTY_LICENSES.txt",
            include_str!("../../../THIRD_PARTY_LICENSES.txt"),
        ),
    ] {
        std::fs::write(directory.join(name), contents).map_err(|e| e.to_string())?;
    }
    tauri_plugin_opener::open_path(&directory, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pending_recordings(
    webview: tauri::Webview,
    paths: State<'_, Arc<crate::portable_paths::PortablePaths>>,
) -> Result<Vec<crate::recovery::PendingRecording>, String> {
    ensure_main(&webview)?;
    crate::recovery::list(&paths.working).map_err(|cause| format!("{cause:#}"))
}

#[tauri::command]
pub fn open_recovery_folder(
    webview: tauri::Webview,
    paths: State<'_, Arc<crate::portable_paths::PortablePaths>>,
) -> Result<(), String> {
    ensure_main(&webview)?;
    tauri_plugin_opener::open_path(&paths.working, None::<&str>).map_err(|cause| cause.to_string())
}

#[tauri::command]
pub async fn retry_recording_save(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    name: String,
    paths: State<'_, Arc<crate::portable_paths::PortablePaths>>,
    history: State<'_, Arc<HistoryStore>>,
) -> Result<bool, String> {
    ensure_main(&webview)?;
    crate::recovery::validate_name(&name).map_err(|cause| cause.to_string())?;
    let source = paths.working.join(&name);
    if !source.is_file()
        || source
            .symlink_metadata()
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("未保存の録画が見つかりません。".into());
    }
    let Some(FilePath::Path(folder)) = app.dialog().file().blocking_pick_folder() else {
        return Ok(false);
    };
    let history = history.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<bool> {
        let destination = folder.join(&name);
        let size = std::fs::metadata(&source)?.len();
        crate::recovery::publish(&source, &destination)?;
        history.add(HistoryEntry {
            path: destination.to_string_lossy().into_owned(),
            file_name: name,
            target_name: "再保存した録画（時間情報なし）".into(),
            created_at: chrono::Local::now().to_rfc3339(),
            duration_seconds: 0,
            size_bytes: size,
        })?;
        Ok(true)
    })
    .await
    .map_err(|cause| cause.to_string())?
    .map_err(|cause| format!("{cause:#}"))
}

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
        settings_warning: settings.warning(),
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
