mod commands;
mod history;
mod legacy_cleanup;
mod models;
mod portable_paths;
mod process_manager;
mod recording;
mod settings;
mod share;
mod share_server;
mod sources;

use history::HistoryStore;
use portable_paths::PortablePaths;
use process_manager::ProcessManager;
use recording::RecordingManager;
use settings::SettingsStore;
use share::{SHARE_SOURCE_ID, SHARE_WINDOW_LABEL, ShareManager};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Emitter, Manager, WebviewWindowBuilder, WindowEvent};

pub fn run() {
    let paths = PortablePaths::initialize().unwrap_or_else(|cause| {
        panic!("Purple Captureのポータブルデータを初期化できません: {cause:#}")
    });
    legacy_cleanup::remove_empty_default_appdata(&paths);
    // WebView2のCookie、localStorage、IndexedDB、キャッシュをPortableDataへ固定する。
    unsafe {
        std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &paths.webview);
    }
    let settings = Arc::new(SettingsStore::load(paths.clone()).unwrap_or_else(|cause| {
        paths.log(
            "ERROR",
            format!("Settings initialization failed: {cause:#}"),
        );
        panic!("設定を初期化できません: {cause:#}")
    }));
    let history = Arc::new(HistoryStore::load(paths.clone()).unwrap_or_else(|cause| {
        paths.log("ERROR", format!("History initialization failed: {cause:#}"));
        panic!("録画履歴を初期化できません: {cause:#}")
    }));
    let recording = Arc::new(RecordingManager::new(paths.clone(), history.clone()));
    let share = Arc::new(ShareManager::new(paths.clone()));
    let processes = Arc::new(ProcessManager::default());
    let shutting_down = Arc::new(AtomicBool::new(false));

    let shutdown_flag = shutting_down.clone();
    let app = tauri::Builder::default()
        // Tauriの要件どおりsingle-instanceを最初のプラグインとして登録する。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(paths.clone())
        .manage(settings)
        .manage(history)
        .manage(recording)
        .manage(share)
        .manage(processes)
        .manage(shutting_down)
        .setup(|app| {
            let paths = app.state::<Arc<PortablePaths>>().inner().clone();
            let main_config = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "main")
                .expect("mainウィンドウ設定がありません。");
            WebviewWindowBuilder::from_config(app.handle(), main_config)?
                .data_directory(paths.webview.clone())
                .build()?;
            legacy_cleanup::remove_empty_default_appdata(&paths);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::list_capture_sources,
            commands::list_microphones,
            commands::recording_history,
            commands::open_recording,
            commands::open_recordings_folder,
            commands::choose_save_folder,
            commands::save_settings,
            commands::start_recording,
            commands::pause_recording,
            commands::resume_recording,
            commands::stop_recording,
            commands::recording_status,
            commands::share_open,
            commands::share_status,
            commands::share_session_info,
            commands::share_browser_options,
            commands::share_launch_browser,
            commands::share_stop,
            commands::share_report_state,
            commands::share_focus_main
        ])
        .on_window_event(move |window, event| {
            if window.label() == SHARE_WINDOW_LABEL && matches!(event, WindowEvent::Destroyed) {
                let app = window.app_handle().clone();
                let share = app.state::<Arc<ShareManager>>().inner().clone();
                let recording = app.state::<Arc<RecordingManager>>().inner().clone();
                share.mark_window_closed(&app);
                std::thread::Builder::new()
                    .name("purplecapture-share-ended".into())
                    .spawn(move || {
                        if let Ok(Some(status)) = recording.stop_if_source(
                            SHARE_SOURCE_ID,
                            "タブ共有ウィンドウが閉じられたため、安全に録画を停止しました。",
                        ) {
                            let _ = app.emit("recording-state", &status);
                        }
                    })
                    .ok();
                return;
            }
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if shutdown_flag.swap(true, Ordering::AcqRel) {
                    return;
                }
                api.prevent_close();
                let _ = window.hide();
                let app = window.app_handle().clone();
                let recording = app.state::<Arc<RecordingManager>>().inner().clone();
                let share = app.state::<Arc<ShareManager>>().inner().clone();
                let processes = app.state::<Arc<ProcessManager>>().inner().clone();
                let paths = app.state::<Arc<PortablePaths>>().inner().clone();
                paths.log("INFO", "Shutdown requested");
                std::thread::Builder::new()
                    .name("purplecapture-shutdown".into())
                    .spawn(move || {
                        paths.log("INFO", "Shutdown step: recording");
                        recording.stop_for_shutdown();
                        paths.log("INFO", "Shutdown step: share");
                        share.close_all(&app);
                        paths.log("INFO", "Shutdown step: child processes");
                        processes.stop_all();
                        paths.log("INFO", "Shutdown step: temporary files");
                        if let Err(cause) = paths.cleanup_stale_working_files() {
                            paths.log("ERROR", format!("Temporary cleanup failed: {cause:#}"));
                        }
                        legacy_cleanup::remove_empty_default_appdata(&paths);
                        paths.log("INFO", "Shutdown step: app exit");
                        app.exit(0);
                    })
                    .ok();
            }
        })
        .build(tauri::generate_context!())
        .expect("Purple Captureを起動できません。");
    paths.log("INFO", "Purple Capture started");
    let exit_paths = paths.clone();
    app.run(move |_app, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            paths.log("INFO", "Purple Capture exited");
        }
    });
    legacy_cleanup::remove_empty_default_appdata(&exit_paths);
}
