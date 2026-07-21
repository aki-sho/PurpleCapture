mod app;
mod recording;
mod share;

pub use app::*;
pub use recording::*;
pub use share::*;

pub fn ensure_main(webview: &tauri::Webview) -> Result<(), String> {
    if webview.label() != "main" {
        return Err("外部Webページからアプリ機能は実行できません。".into());
    }
    Ok(())
}

pub fn ensure_share(webview: &tauri::Webview) -> Result<(), String> {
    if webview.label() != crate::share::SHARE_WINDOW_LABEL {
        return Err("タブ共有ウィンドウ以外から共有状態は変更できません。".into());
    }
    Ok(())
}
