use crate::config_dir::config_dir;
use krad_domain::{PersistedLayout, LAYOUT_SCHEMA_VERSION};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;

const LAYOUT_FILE_NAME: &str = "layout.json";
const TMP_EXTENSION: &str = "json.tmp";
const PERSIST_THREAD_NAME: &str = "krad-ade-layout-writer";

fn layout_file_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(LAYOUT_FILE_NAME))
}

#[must_use]
pub fn read_layout_snapshot() -> Option<PersistedLayout> {
    let bytes = std::fs::read(layout_file_path()?).ok()?;
    known_layout(&bytes)
}

fn known_layout(bytes: &[u8]) -> Option<PersistedLayout> {
    let snapshot: PersistedLayout = serde_json::from_slice(bytes).ok()?;
    (snapshot.version <= LAYOUT_SCHEMA_VERSION).then_some(snapshot)
}

pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(TMP_EXTENSION);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

pub struct LayoutWriter {
    queue: Option<Sender<Vec<u8>>>,
    worker: Option<JoinHandle<()>>,
}

impl LayoutWriter {
    #[must_use]
    pub fn spawn() -> Option<Self> {
        Self::spawn_at(layout_file_path()?)
    }

    fn spawn_at(path: PathBuf) -> Option<Self> {
        let (queue, inbox) = channel::<Vec<u8>>();
        let worker = std::thread::Builder::new()
            .name(PERSIST_THREAD_NAME.to_owned())
            .spawn(move || {
                while let Ok(mut bytes) = inbox.recv() {
                    while let Ok(newer) = inbox.try_recv() {
                        bytes = newer;
                    }
                    drop(write_atomically(&path, &bytes));
                }
            })
            .ok()?;
        Some(Self {
            queue: Some(queue),
            worker: Some(worker),
        })
    }

    pub fn persist(&self, snapshot: &PersistedLayout) {
        let (Some(queue), Ok(bytes)) = (&self.queue, serde_json::to_vec_pretty(snapshot)) else {
            return;
        };
        drop(queue.send(bytes));
    }
}

impl Drop for LayoutWriter {
    fn drop(&mut self) {
        self.queue = None;
        if let Some(worker) = self.worker.take() {
            drop(worker.join());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::EnvironmentName;

    #[test]
    fn a_layout_from_a_newer_app_is_not_loaded() -> serde_json::Result<()> {
        let layout = |version| PersistedLayout {
            version,
            active: EnvironmentName::new("main".to_owned()),
            environments: Vec::new(),
        };
        let current = serde_json::to_vec(&layout(LAYOUT_SCHEMA_VERSION))?;
        assert_eq!(
            known_layout(&current).map(|loaded| loaded.version),
            Some(LAYOUT_SCHEMA_VERSION)
        );
        let newer = serde_json::to_vec(&layout(LAYOUT_SCHEMA_VERSION + 1))?;
        assert!(known_layout(&newer).is_none());
        Ok(())
    }

    #[test]
    fn writer_lands_the_newest_snapshot() {
        let dir = std::env::temp_dir().join(format!("krad-ade-writer-{}", std::process::id()));
        let path = dir.join(LAYOUT_FILE_NAME);
        let snapshot = |name: &str| PersistedLayout {
            version: LAYOUT_SCHEMA_VERSION,
            active: EnvironmentName::new(name.to_owned()),
            environments: Vec::new(),
        };
        let writer = LayoutWriter::spawn_at(path.clone());
        assert!(writer.is_some(), "a writer thread starts");
        if let Some(writer) = writer {
            writer.persist(&snapshot("older"));
            writer.persist(&snapshot("newer"));
        }
        let written = std::fs::read(&path).unwrap_or_default();
        let decoded = serde_json::from_slice::<PersistedLayout>(&written);
        assert!(decoded.is_ok_and(|layout| layout.active.as_str() == "newer"));
        assert!(std::fs::remove_dir_all(&dir).is_ok());
    }
}
