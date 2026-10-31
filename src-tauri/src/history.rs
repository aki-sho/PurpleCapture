use crate::{json_store, models::HistoryEntry, portable_paths::PortablePaths};
use anyhow::Result;
use parking_lot::RwLock;
use std::{path::Path, sync::Arc};

pub struct HistoryStore {
    paths: Arc<PortablePaths>,
    entries: RwLock<Vec<HistoryEntry>>,
    warning: Option<String>,
}

impl HistoryStore {
    pub fn load(paths: Arc<PortablePaths>) -> Result<Self> {
        let file = paths.data.join("history.json");
        let loaded = json_store::load(&file, "録画履歴", Vec::<HistoryEntry>::new, |_| Ok(()))?;
        if let Some(warning) = &loaded.warning {
            paths.log("WARN", warning);
        }
        Ok(Self {
            paths,
            entries: RwLock::new(loaded.value),
            warning: loaded.warning,
        })
    }

    pub fn warning(&self) -> Option<String> {
        self.warning.clone()
    }

    pub fn list(&self) -> Vec<HistoryEntry> {
        let mut entries = self.entries.write();
        entries.retain(|entry| Path::new(&entry.path).is_file());
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        entries.clone()
    }

    pub fn contains_path(&self, path: &Path) -> bool {
        let canonical = path.canonicalize().ok();
        self.entries.read().iter().any(|entry| {
            canonical
                .as_ref()
                .zip(Path::new(&entry.path).canonicalize().ok().as_ref())
                .is_some_and(|(left, right)| left == right)
        })
    }

    pub fn add(&self, entry: HistoryEntry) -> Result<()> {
        let mut entries = self.entries.write();
        let mut updated = entries.clone();
        updated.insert(0, entry);
        let file = self.paths.data.join("history.json");
        json_store::save(&file, &updated)?;
        *entries = updated;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn entry(path: &Path) -> HistoryEntry {
        HistoryEntry {
            path: path.to_string_lossy().into_owned(),
            file_name: "recording.mp4".into(),
            target_name: "test".into(),
            created_at: "2026-10-31".into(),
            duration_seconds: 1,
            size_bytes: 1,
        }
    }

    #[test]
    fn corrupt_history_recovers_backup_and_failed_save_preserves_memory() {
        let root = tempfile::tempdir().unwrap();
        let paths = PortablePaths::from_root(root.path().to_owned()).unwrap();
        let video = paths.recordings.join("recording.mp4");
        fs::write(&video, b"video").unwrap();
        let store = HistoryStore::load(paths.clone()).unwrap();
        store.add(entry(&video)).unwrap();
        store.add(entry(&video)).unwrap();
        let file = paths.data.join("history.json");
        fs::write(&file, b"[").unwrap();
        let recovered = HistoryStore::load(paths.clone()).unwrap();
        assert_eq!(recovered.list().len(), 1);
        assert!(recovered.warning().is_some());
        fs::rename(&file, paths.data.join("previous.json")).unwrap();
        fs::create_dir(&file).unwrap();
        assert!(recovered.add(entry(&video)).is_err());
        assert_eq!(recovered.list().len(), 1);
    }
}
