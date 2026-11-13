use krad_domain::UploadExisting;
use krad_host::{HostLink, UploadProgress};
use krad_protocol::{Answer, EventKind, RequestId, UploadShown};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::failed;

#[derive(Debug)]
struct Running {
    progress: Arc<UploadProgress>,
    shown: (u64, u64),
}

#[derive(Debug, Default)]
pub(crate) struct Uploads {
    running: HashMap<RequestId, Running>,
}

impl Uploads {
    pub(crate) fn start(&mut self, upload: RequestId) -> Arc<UploadProgress> {
        let progress = UploadProgress::new();
        self.running.insert(
            upload,
            Running {
                progress: Arc::clone(&progress),
                shown: (0, 0),
            },
        );
        progress
    }

    pub(crate) fn cancel(&self, upload: RequestId) -> bool {
        self.running
            .get(&upload)
            .map(|running| running.progress.cancel())
            .is_some()
    }

    pub(crate) fn finished(&mut self, upload: RequestId, outbox: &mut Vec<EventKind>) {
        self.tick(outbox);
        self.running.remove(&upload);
    }

    pub(crate) fn tick(&mut self, outbox: &mut Vec<EventKind>) {
        for (upload, running) in &mut self.running {
            let now = (running.progress.sent(), running.progress.total());
            if now == running.shown {
                continue;
            }
            running.shown = now;
            outbox.push(EventKind::UploadProgress(UploadShown {
                upload: *upload,
                sent: now.0,
                total: now.1,
            }));
        }
    }
}

pub(crate) fn upload(
    link: &HostLink,
    (dir, sources): (&Path, &[PathBuf]),
    existing: UploadExisting,
    progress: &UploadProgress,
) -> Answer {
    sources
        .iter()
        .try_for_each(|source| link.upload_with(source, dir, progress, existing).map(drop))
        .map_or_else(|error| failed(&error.into()), |()| Answer::Done)
}
