use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const PREFIX: &str = "krad-engine-tools";

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct Scratch(PathBuf);

impl Scratch {
    pub(super) fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or_default(|elapsed| elapsed.as_nanos());
        let at = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("{PREFIX}-{}-{nanos}-{at}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap_or_default();
        Self(dir)
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }

    pub(super) fn file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_default();
        }
        std::fs::write(&path, content).unwrap_or_default();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap_or_default();
    }
}
