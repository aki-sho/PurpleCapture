use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub struct Loaded<T> {
    pub value: T,
    pub warning: Option<String>,
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn read<T: DeserializeOwned>(path: &Path, validate: &impl Fn(&T) -> Result<()>) -> Result<T> {
    let value = serde_json::from_slice(&fs::read(path)?)?;
    validate(&value)?;
    Ok(value)
}

fn quarantine(path: &Path) -> Result<()> {
    if path.try_exists()? {
        fs::rename(
            path,
            path.with_extension(format!("json.corrupt-{}", uuid::Uuid::new_v4().simple())),
        )
        .with_context(|| format!("破損ファイルを退避できません: {}", path.display()))?;
    }
    Ok(())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("保存先フォルダがありません。")?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    staged.persist(path).map_err(|error| error.error)?;
    Ok(())
}

// Preserve the previous readable version before replacing the live file.
pub fn save<T: Serialize + DeserializeOwned>(path: &Path, value: &T) -> Result<()> {
    let output = serde_json::to_vec_pretty(value)?;
    let previous = match fs::read(path) {
        Ok(bytes) => {
            let _: T = serde_json::from_slice(&bytes)
                .context("保存済みデータが破損しています。再起動して復旧してください。")?;
            bytes
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => output.clone(),
        Err(error) => return Err(error.into()),
    };
    atomic_write(&backup_path(path), &previous)?;
    atomic_write(path, &output)
}

pub fn load<T: Serialize + DeserializeOwned>(
    path: &Path,
    label: &str,
    default: impl FnOnce() -> T,
    validate: impl Fn(&T) -> Result<()>,
) -> Result<Loaded<T>> {
    let backup = backup_path(path);
    let primary_exists = path.try_exists()?;
    if primary_exists {
        if let Ok(value) = read(path, &validate) {
            if !backup.try_exists()? {
                atomic_write(&backup, &serde_json::to_vec_pretty(&value)?)?;
            }
            return Ok(Loaded {
                value,
                warning: None,
            });
        }
        quarantine(path)?;
    }
    let backup_exists = backup.try_exists()?;
    let (value, recovered) = if backup_exists {
        match read(&backup, &validate) {
            Ok(value) => (value, true),
            Err(_) => {
                quarantine(&backup)?;
                (default(), false)
            }
        }
    } else {
        (default(), false)
    };
    validate(&value)?;
    let output = serde_json::to_vec_pretty(&value)?;
    atomic_write(path, &output)?;
    if !recovered {
        atomic_write(&backup, &output)?;
    }
    let warning = (primary_exists || backup_exists).then(|| {
        let action = if recovered { "バックアップから復旧しました" } else { "既定値で再作成しました" };
        format!("{label}ファイルが破損または見つからないため、{action}。破損ファイルは同じフォルダへ退避しています。")
    });
    Ok(Loaded { value, warning })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncated_primary_recovers_previous_version_and_keeps_evidence() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("data.json");
        save(&path, &vec![1]).unwrap();
        save(&path, &vec![1, 2]).unwrap();
        fs::write(&path, b"[").unwrap();
        let loaded = load(&path, "テスト", Vec::<u32>::new, |_| Ok(())).unwrap();
        assert_eq!(loaded.value, vec![1]);
        assert!(loaded.warning.is_some());
        let files: Vec<_> = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert!(
            files
                .iter()
                .any(|file| file.to_string_lossy().contains(".corrupt-")
                    && fs::read(file).unwrap() == b"[")
        );
        assert_eq!(read::<Vec<u32>>(&path, &|_| Ok(())).unwrap(), vec![1]);
    }

    #[test]
    fn missing_primary_recovers_backup() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("data.json");
        fs::write(backup_path(&path), b"[7]").unwrap();
        let loaded = load(&path, "テスト", Vec::<u32>::new, |_| Ok(())).unwrap();
        assert_eq!(loaded.value, vec![7]);
        assert!(loaded.warning.is_some());
    }

    #[test]
    fn both_invalid_files_are_retained_and_defaults_are_written() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("data.json");
        fs::write(&path, b"broken primary").unwrap();
        fs::write(backup_path(&path), b"broken backup").unwrap();
        let loaded = load(&path, "テスト", || vec![3_u32], |_| Ok(())).unwrap();
        assert_eq!(loaded.value, vec![3]);
        assert!(loaded.warning.is_some());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 4);
    }
}
