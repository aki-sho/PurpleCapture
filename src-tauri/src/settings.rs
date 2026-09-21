use crate::{models::AppSettings, portable_paths::PortablePaths};
use anyhow::{Context, Result, bail};
use parking_lot::RwLock;
use std::{fs, sync::Arc};

pub struct SettingsStore {
    paths: Arc<PortablePaths>,
    value: RwLock<AppSettings>,
}

impl SettingsStore {
    pub fn load(paths: Arc<PortablePaths>) -> Result<Self> {
        let file = paths.settings.join("settings.json");
        let mut value = if file.exists() {
            serde_json::from_slice(&fs::read(&file)?)
                .with_context(|| format!("設定ファイルが壊れています: {}", file.display()))?
        } else {
            AppSettings {
                save_directory: paths.recordings.to_string_lossy().into_owned(),
                quality: "standard".into(),
                fps: 30,
                system_audio: true,
                microphone: false,
                microphone_id: None,
            }
        };
        Self::validate(&value)?;
        // A disconnected destination must not prevent access to retained recordings.
        value.save_directory = match paths.ensure_absolute_directory(&value.save_directory) {
            Ok(directory) => directory,
            Err(cause) => {
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
        };
        store.persist()?;
        Ok(store)
    }

    pub fn get(&self) -> AppSettings {
        self.value.read().clone()
    }

    pub fn save(&self, mut settings: AppSettings) -> Result<AppSettings> {
        Self::validate(&settings)?;
        settings.save_directory = self
            .paths
            .ensure_absolute_directory(&settings.save_directory)?
            .to_string_lossy()
            .into_owned();
        *self.value.write() = settings.clone();
        self.persist()?;
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
        let output = serde_json::to_vec_pretty(&*self.value.read())?;
        let target = self.paths.settings.join("settings.json");
        fs::write(target, output)?;
        Ok(())
    }
}
