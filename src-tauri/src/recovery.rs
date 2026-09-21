use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::{fs, io, path::Path};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRecording {
    pub name: String,
    pub size_bytes: u64,
    pub ready: bool,
}

pub fn list(directory: &Path) -> Result<Vec<PendingRecording>> {
    let mut result = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".mp4") || name.ends_with(".mp4.part") {
            result.push(PendingRecording {
                ready: name.ends_with(".mp4"),
                name,
                size_bytes: entry.metadata()?.len(),
            });
        }
    }
    result.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(result)
}

pub fn validate_name(name: &str) -> Result<()> {
    if !name.starts_with("PurpleCapture_")
        || !name.ends_with(".mp4")
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        || name.contains("..")
    {
        bail!("未保存の録画名が不正です。");
    }
    Ok(())
}

// Stage on the destination volume, sync, then publish without replacing a file.
// The original is removed only after the complete destination exists.
pub fn publish(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .context("保存先フォルダがありません。")?;
    fs::create_dir_all(parent)?;
    let mut input = fs::File::open(source)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    io::copy(&mut input, staged.as_file_mut())?;
    staged.as_file().sync_all()?;
    staged
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    drop(input);
    fs::remove_file(source)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_save_preserves_recording_and_retry_succeeds() {
        let directory = tempfile::tempdir().unwrap();
        let working = directory.path().join("working");
        fs::create_dir(&working).unwrap();
        let source = working.join("PurpleCapture_test.mp4");
        fs::write(&source, b"finalized recording").unwrap();
        let blocked = directory.path().join("blocked");
        fs::write(&blocked, b"not a directory").unwrap();
        assert!(publish(&source, &blocked.join("output.mp4")).is_err());
        assert_eq!(list(&working).unwrap().len(), 1);
        assert_eq!(fs::read(&source).unwrap(), b"finalized recording");
        let destination = directory.path().join("output.mp4");
        publish(&source, &destination).unwrap();
        assert_eq!(fs::read(destination).unwrap(), b"finalized recording");
        assert!(!source.exists());
    }
    #[test]
    fn existing_destination_is_not_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.mp4");
        let destination = directory.path().join("existing.mp4");
        fs::write(&source, b"new").unwrap();
        fs::write(&destination, b"old").unwrap();
        assert!(publish(&source, &destination).is_err());
        assert_eq!(fs::read(source).unwrap(), b"new");
        assert_eq!(fs::read(destination).unwrap(), b"old");
    }
    #[test]
    fn partial_files_are_preserved_but_not_offered_as_ready() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("PurpleCapture_crash.mp4.part"),
            b"partial",
        )
        .unwrap();
        assert!(!list(directory.path()).unwrap()[0].ready);
        for invalid in [
            "../PurpleCapture_x.mp4",
            "C:\\x.mp4",
            "PurpleCapture_x.mp4:stream",
            "PurpleCapture_x.mp4.part",
        ] {
            assert!(validate_name(invalid).is_err());
        }
    }
}
