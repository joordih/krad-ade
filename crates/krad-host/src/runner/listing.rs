use krad_domain::{CloudProcessId, CloudProcessRun};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use super::journal::Journal;
use super::process::Tracked;
use super::status::CloudProcessStatus;

pub(super) fn list_processes(
    processes_dir: &Path,
    tracked: &HashMap<CloudProcessId, Arc<Tracked>>,
) -> Vec<CloudProcessStatus> {
    let mut statuses: Vec<CloudProcessStatus> = tracked
        .iter()
        .map(|(id, tracked)| CloudProcessStatus {
            id: id.clone(),
            run: tracked.run(),
            end: tracked.written(),
        })
        .collect();
    statuses.extend(
        journaled_ids(processes_dir)
            .filter(|id| !tracked.contains_key(id))
            .filter_map(|id| {
                let end = Journal::new(processes_dir.join(id.as_str())).len().ok()?;
                Some(CloudProcessStatus {
                    id,
                    run: CloudProcessRun::Ended(None),
                    end,
                })
            }),
    );
    statuses.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
    statuses
}

fn journaled_ids(processes_dir: &Path) -> impl Iterator<Item = CloudProcessId> {
    fs::read_dir(processes_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
}
