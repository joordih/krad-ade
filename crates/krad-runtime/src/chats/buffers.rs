use krad_chat::acp::OpenBuffers;
use krad_domain::EnvironmentName;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

struct DiskOnly;

impl OpenBuffers for DiskOnly {
    fn unsaved_text(&self, _: &Path) -> Option<String> {
        None
    }

    fn written(&self, _: &Path, _: &str) {}
}

#[derive(Clone)]
pub(crate) struct OfferedBuffers(pub(crate) Arc<dyn OpenBuffers>);

impl fmt::Debug for OfferedBuffers {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OfferedBuffers")
    }
}

#[derive(Debug, Default)]
pub(crate) struct EditorBuffers {
    offered: HashMap<EnvironmentName, OfferedBuffers>,
}

impl EditorBuffers {
    pub(crate) fn offer(&mut self, environment: EnvironmentName, buffers: OfferedBuffers) {
        self.offered.insert(environment, buffers);
    }

    pub(crate) fn of(&self, environment: &EnvironmentName) -> Arc<dyn OpenBuffers> {
        self.offered.get(environment).map_or_else(
            || Arc::new(DiskOnly) as Arc<dyn OpenBuffers>,
            |offered| Arc::clone(&offered.0),
        )
    }
}
