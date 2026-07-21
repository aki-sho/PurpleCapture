use crate::{models::HistoryEntry, portable_paths::PortablePaths};
use anyhow::Result;
use parking_lot::RwLock;
use std::{fs, path::Path, sync::Arc};

pub struct HistoryStore {
    paths: Arc<PortablePaths>,
    entries: RwLock<Vec<HistoryEntry>>,
}

impl HistoryStore {
    pub fn load(paths: Arc<PortablePaths>) -> Result<Self> {
        let file = paths.data.join("history.json");
        let entries = if file.exists() {
            serde_json::from_slice(&fs::read(file)?).unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(Self {
            paths,
            entries: RwLock::new(entries),
        })
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
        self.entries.write().insert(0, entry);
        self.persist()
    }

    fn persist(&self) -> Result<()> {
        let file = self.paths.data.join("history.json");
        fs::write(file, serde_json::to_vec_pretty(&*self.entries.read())?)?;
        Ok(())
    }
}
