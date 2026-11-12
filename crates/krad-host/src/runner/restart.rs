use krad_domain::CloudProcessId;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use super::journal::Journal;
use super::listing::journaled_ids;
use super::piped::adopt_piped;
use super::process::{group_alive, stop_group, Tracked};

const ORPHAN_STOP_THREAD: &str = "krad-cloud-orphan-stop";

pub(super) fn adopt_previous(processes_dir: &Path) -> HashMap<CloudProcessId, Arc<Tracked>> {
    let mut adopted = HashMap::new();
    for id in journaled_ids(processes_dir) {
        let journal = Journal::new(processes_dir.join(id.as_str()));
        if journal.exit_code().is_some() || !journal.exists() {
            continue;
        }
        let Some(group) = journal.pid().filter(|group| group_alive(*group)) else {
            continue;
        };
        if journal.is_terminal() {
            stop_orphan(&id, group);
            continue;
        }
        let Some(tracked) = adopt_piped(journal, group) else {
            tracing::debug!(%id, "a cloud process from an earlier run was not adopted");
            continue;
        };
        tracing::info!(%id, "adopted a cloud process from an earlier run");
        adopted.insert(id, tracked);
    }
    adopted
}

fn stop_orphan(id: &CloudProcessId, group: u32) {
    tracing::info!(%id, "stopping a cloud terminal left by an earlier run");
    let spawned = thread::Builder::new()
        .name(ORPHAN_STOP_THREAD.to_owned())
        .spawn(move || stop_group(group));
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not stop a cloud terminal left by an earlier run");
    }
}
