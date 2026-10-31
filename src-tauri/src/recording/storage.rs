use anyhow::{Context, Result, bail};
use std::{fs, path::Path};

pub const RESERVE_BYTES: u64 = 256 * 1024 * 1024;

fn require_space(available: std::io::Result<u64>, required: u64, label: &str) -> Result<()> {
    let available = available.with_context(|| format!("{label}の空き容量を確認できません。"))?;
    if available < required {
        bail!(
            "{label}の空き容量が不足しています（必要な空き容量: {} MB）。",
            required.div_ceil(1024 * 1024)
        );
    }
    Ok(())
}

pub fn check_start(working_directory: &Path, destination: &Path) -> Result<()> {
    require_space(
        fs2::available_space(working_directory),
        RESERVE_BYTES,
        "一時録画先",
    )?;
    require_space(fs2::available_space(destination), RESERVE_BYTES, "保存先")
}

pub fn check_recording(working_file: &Path, destination: &Path) -> Result<()> {
    check_recording_with(working_file, destination, |path| fs2::available_space(path))
}

fn check_recording_with(
    working_file: &Path,
    destination: &Path,
    mut available: impl FnMut(&Path) -> std::io::Result<u64>,
) -> Result<()> {
    let working_directory = working_file.parent().context("一時録画先がありません。")?;
    let destination = destination.parent().context("保存先がありません。")?;
    require_space(available(working_directory), RESERVE_BYTES, "一時録画先")?;
    let size = fs::metadata(working_file)
        .context("一時録画データを確認できません。")?
        .len();
    // Publishing stages a full copy before deleting the source, even on one volume.
    require_space(
        available(destination),
        size.saturating_add(RESERVE_BYTES),
        "保存先",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[test]
    fn failed_capacity_query_is_not_treated_as_unlimited() {
        assert!(
            require_space(
                Err(Error::from(ErrorKind::PermissionDenied)),
                RESERVE_BYTES,
                "test"
            )
            .is_err()
        );
        assert!(require_space(Ok(RESERVE_BYTES - 1), RESERVE_BYTES, "test").is_err());
        assert!(require_space(Ok(RESERVE_BYTES), RESERVE_BYTES, "test").is_ok());
    }

    #[test]
    fn monitors_working_volume_and_reserves_space_for_the_complete_copy() {
        let root = tempfile::tempdir().unwrap();
        let working = root.path().join("working");
        let destination = root.path().join("destination");
        fs::create_dir_all(&working).unwrap();
        fs::create_dir_all(&destination).unwrap();
        let source = working.join("recording.mp4.part");
        fs::write(&source, [0; 1024]).unwrap();
        let output = destination.join("recording.mp4");
        assert!(
            check_recording_with(&source, &output, |path| Ok(if path == working {
                RESERVE_BYTES - 1
            } else {
                u64::MAX
            }))
            .is_err()
        );
        assert!(check_recording_with(&source, &output, |_| Ok(RESERVE_BYTES + 1023)).is_err());
        assert!(check_recording_with(&source, &output, |_| Ok(RESERVE_BYTES + 1024)).is_ok());
        assert!(
            check_recording_with(&source, &output, |path| if path == destination {
                Err(Error::from(ErrorKind::NotFound))
            } else {
                Ok(u64::MAX)
            })
            .is_err()
        );
    }
}
