use std::path::Path;

pub trait OpenBuffers: Send + Sync + 'static {
    fn unsaved_text(&self, path: &Path) -> Option<String>;
    fn written(&self, path: &Path, content: &str);
}
