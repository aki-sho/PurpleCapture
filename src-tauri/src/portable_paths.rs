use anyhow::{Context, Result, bail};
use chrono::Local;
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Arc,
};

#[derive(Debug, Clone)]
pub struct PortablePaths {
    pub root: PathBuf,
    pub settings: PathBuf,
    pub data: PathBuf,
    pub recordings: PathBuf,
    pub logs: PathBuf,
    pub cache: PathBuf,
    pub webview: PathBuf,
    pub temp: PathBuf,
    pub working: PathBuf,
    pub bin: PathBuf,
}

impl PortablePaths {
    pub fn initialize() -> Result<Arc<Self>> {
        let root = if cfg!(debug_assertions) {
            let data_name = env::var("PURPLECAPTURE_TEST_DATA_NAME")
                .unwrap_or_else(|_| "PurpleCapture-PortableData".into());
            if data_name.is_empty()
                || data_name.len() > 80
                || !data_name
                    .chars()
                    .all(|value| value.is_ascii_alphanumeric() || value == '-' || value == '_')
            {
                bail!("開発用データフォルダ名が不正です。");
            }
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .context("開発用データの基準フォルダを取得できません。")?
                .join(".devdata")
                .join(data_name)
        } else {
            env::current_exe()
                .context("実行ファイルの場所を取得できません。")?
                .parent()
                .context("実行ファイルの親フォルダを取得できません。")?
                .join("PurpleCapture-PortableData")
        };
        Self::from_root(root)
    }

    fn from_root(root: PathBuf) -> Result<Arc<Self>> {
        let paths = Self {
            settings: root.join("settings"),
            data: root.join("data"),
            recordings: root.join("data").join("recordings"),
            logs: root.join("logs"),
            cache: root.join("cache"),
            webview: root.join("cache").join("webview"),
            temp: root.join("temp"),
            working: root.join("temp").join("working"),
            bin: root.join("temp").join("bin"),
            root,
        };
        for directory in [
            &paths.root,
            &paths.settings,
            &paths.data,
            &paths.recordings,
            &paths.logs,
            &paths.cache,
            &paths.webview,
            &paths.temp,
            &paths.working,
            &paths.bin,
        ] {
            fs::create_dir_all(directory)
                .with_context(|| format!("フォルダを作成できません: {}", directory.display()))?;
        }
        paths.verify_writable()?;
        Ok(Arc::new(paths))
    }

    fn verify_writable(&self) -> Result<()> {
        let probe = self.temp.join(".write-test");
        fs::write(&probe, b"PurpleCapture").with_context(|| {
            format!(
                "PortableDataへ書き込めません。書き込み可能な場所へアプリを移動してください: {}",
                self.root.display()
            )
        })?;
        fs::remove_file(probe).ok();
        Ok(())
    }

    pub fn log(&self, level: &str, message: impl AsRef<str>) {
        let file = self.logs.join(format!(
            "purplecapture-{}.log",
            Local::now().format("%Y-%m-%d")
        ));
        if let Ok(mut output) = OpenOptions::new().create(true).append(true).open(file) {
            let sanitized = message.as_ref().replace(['\r', '\n'], " ");
            let _ = writeln!(
                output,
                "{} [{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                level,
                sanitized
            );
        }
    }

    pub fn ensure_absolute_directory(&self, value: &str) -> Result<PathBuf> {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            bail!("保存先には絶対パスを指定してください。");
        }
        fs::create_dir_all(&path)
            .with_context(|| format!("保存先を作成できません: {}", path.display()))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initialization_preserves_completed_and_partial_recordings() {
        let directory = tempfile::tempdir().unwrap();
        let paths = PortablePaths::from_root(directory.path().to_owned()).unwrap();
        fs::write(paths.working.join("PurpleCapture_saved.mp4"), b"completed").unwrap();
        fs::write(
            paths.working.join("PurpleCapture_crash.mp4.part"),
            b"partial",
        )
        .unwrap();
        drop(paths);
        let reopened = PortablePaths::from_root(directory.path().to_owned()).unwrap();
        assert_eq!(
            fs::read(reopened.working.join("PurpleCapture_saved.mp4")).unwrap(),
            b"completed"
        );
        assert_eq!(
            fs::read(reopened.working.join("PurpleCapture_crash.mp4.part")).unwrap(),
            b"partial"
        );
    }
}
