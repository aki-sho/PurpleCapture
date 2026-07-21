use crate::portable_paths::PortablePaths;
use std::{env, fs, path::PathBuf};

/// Tauri/Wryの既定パスが過去の起動で作った空フォルダだけを整理する。
/// ファイルが存在する場合はユーザーデータ保護のため削除しない。
pub fn remove_empty_default_appdata(paths: &PortablePaths) {
    let Some(local_app_data) = env::var_os("LOCALAPPDATA") else {
        return;
    };
    let candidate = PathBuf::from(local_app_data).join("com.purplecapture.desktop");
    if !candidate.is_dir() {
        return;
    }
    let is_empty = fs::read_dir(&candidate)
        .ok()
        .and_then(|mut entries| entries.next())
        .is_none();
    if is_empty {
        if let Err(cause) = fs::remove_dir(&candidate) {
            paths.log(
                "WARN",
                format!("Empty legacy AppData folder could not be removed: {cause}"),
            );
        }
    } else {
        paths.log(
            "WARN",
            format!(
                "Legacy AppData folder was not removed because it is not empty: {}",
                candidate.display()
            ),
        );
    }
}
