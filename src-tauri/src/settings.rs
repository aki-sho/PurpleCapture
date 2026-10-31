use crate::{json_store, models::AppSettings, portable_paths::PortablePaths};
use anyhow::{Result, bail};
use parking_lot::RwLock;
use std::sync::Arc;

pub struct SettingsStore {
    paths: Arc<PortablePaths>,
    value: RwLock<AppSettings>,
    warning: Option<String>,
}

impl SettingsStore {
    pub fn load(paths: Arc<PortablePaths>) -> Result<Self> {
        let file = paths.settings.join("settings.json");
        let loaded = json_store::load(
            &file,
            "設定",
            || AppSettings {
                save_directory: paths.recordings.to_string_lossy().into_owned(),
                quality: "standard".into(),
                fps: 30,
                system_audio: true,
                microphone: false,
                microphone_id: None,
            },
            Self::validate,
        )?;
        let mut warning = loaded.warning;
        let mut value = loaded.value;
        // A disconnected destination must not prevent access to retained recordings.
        value.save_directory = match paths.ensure_absolute_directory(&value.save_directory) {
            Ok(directory) => directory,
            Err(cause) => {
                let notice = "保存先が利用できないため、このPCの既定の録画フォルダへ切り替えました。設定から保存先を選び直してください。";
                warning = Some(match warning { Some(previous) => format!("{previous} {notice}"), None => notice.into() });
                paths.log(
                    "WARN",
                    format!("Save directory unavailable; using local recordings: {cause:#}"),
                );
                paths.recordings.clone()
            }
        }
        .to_string_lossy()
        .into_owned();
        let store = Self {
            paths,
            value: RwLock::new(value),
            warning,
        };
        if let Some(warning) = &store.warning {
            store.paths.log("WARN", warning);
        }
        store.persist()?;
        Ok(store)
    }

    pub fn get(&self) -> AppSettings {
        self.value.read().clone()
    }

    pub fn warning(&self) -> Option<String> {
        self.warning.clone()
    }

    pub fn save(&self, mut settings: AppSettings) -> Result<AppSettings> {
        Self::validate(&settings)?;
        settings.save_directory = self
            .paths
            .ensure_absolute_directory(&settings.save_directory)?
            .to_string_lossy()
            .into_owned();
        let mut current = self.value.write();
        self.persist_value(&settings)?;
        *current = settings.clone();
        Ok(settings)
    }

    fn validate(settings: &AppSettings) -> Result<()> {
        if !matches!(settings.fps, 30 | 60) {
            bail!("FPSは30または60を指定してください。");
        }
        if !matches!(settings.quality.as_str(), "standard" | "high") {
            bail!("画質の指定が不正です。");
        }
        if settings.save_directory.trim().is_empty() || settings.save_directory.len() > 1024 {
            bail!("保存先が不正です。");
        }
        if settings
            .microphone_id
            .as_ref()
            .is_some_and(|id| id.len() > 2048)
        {
            bail!("マイクデバイスIDが不正です。");
        }
        Ok(())
    }

    fn persist(&self) -> Result<()> {
        self.persist_value(&self.value.read())
    }

    fn persist_value(&self, value: &AppSettings) -> Result<()> {
        let target = self.paths.settings.join("settings.json");
        json_store::save(&target, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn invalid_settings_restore_validated_backup_without_blocking_startup() {
        let root = tempfile::tempdir().unwrap();
        let paths = PortablePaths::from_root(root.path().to_owned()).unwrap();
        let store = SettingsStore::load(paths.clone()).unwrap();
        let mut invalid = store.get();
        invalid.fps = 500;
        fs::write(
            paths.settings.join("settings.json"),
            serde_json::to_vec(&invalid).unwrap(),
        )
        .unwrap();
        let recovered = SettingsStore::load(paths).unwrap();
        assert_eq!(recovered.get().fps, 30);
        assert!(recovered.warning().is_some());
    }
    #[test]
    fn unavailable_destination_does_not_block_startup_or_reselection() {
        let root = tempfile::tempdir().unwrap();
        let paths = PortablePaths::from_root(root.path().to_owned()).unwrap();
        let initial = SettingsStore::load(paths.clone()).unwrap();
        let mut settings = initial.get();
        let blocked = root.path().join("disconnected-drive");
        fs::write(&blocked, b"unavailable destination").unwrap();
        settings.save_directory = blocked.join("recordings").to_string_lossy().into_owned();
        fs::write(
            paths.settings.join("settings.json"),
            serde_json::to_vec(&settings).unwrap(),
        )
        .unwrap();
        let reopened = SettingsStore::load(paths.clone()).unwrap();
        assert!(reopened.warning().is_some());
        assert_eq!(
            reopened.get().save_directory,
            paths.recordings.to_string_lossy()
        );
        let mut changed = reopened.get();
        changed.save_directory = root
            .path()
            .join("new-destination")
            .to_string_lossy()
            .into_owned();
        reopened.save(changed.clone()).unwrap();
        assert_eq!(
            SettingsStore::load(paths).unwrap().get().save_directory,
            changed.save_directory
        );
    }

    #[test]
    fn failed_settings_write_preserves_last_accepted_settings() {
        let root = tempfile::tempdir().unwrap();
        let paths = PortablePaths::from_root(root.path().to_owned()).unwrap();
        let store = SettingsStore::load(paths.clone()).unwrap();
        let before = store.get();
        let original_file = paths.settings.join("settings.json");
        fs::rename(&original_file, paths.settings.join("previous.json")).unwrap();
        fs::create_dir(&original_file).unwrap();
        let mut changed = before.clone();
        changed.fps = 60;
        assert!(store.save(changed).is_err());
        assert_eq!(store.get().fps, before.fps);
    }
}
