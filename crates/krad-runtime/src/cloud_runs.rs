use krad_chat::CloudRun;
use krad_domain::{
    ClaudeOauthToken, CloudPlacement, CloudProcess, CloudProcessId, CloudRunnerId, Host,
    JournalOffset, Pairing,
};
use krad_host::HostLink;
use krad_protocol::AgentRef;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::cloud::{new_nonce, node_process_id};
use crate::environments::Environments;
use crate::jobs::Jobs;
use crate::runners::{ReadyRunner, Runners};

mod liveness;
mod pty;

pub(crate) use liveness::LivenessChecked;
use liveness::{check, Liveness};
pub(crate) use pty::{CloudProgram, CloudTerminal};

const NO_PROCESS_ID: &str = "The cloud session's folder gives no process name";
pub(crate) const REMOVED_RUNNER: &str = "This cloud runner was removed";
pub(crate) const CONNECTING_PREFIX: &str = "Connecting to cloud runner";

#[derive(Clone, Debug)]
pub(crate) struct CloudSite {
    pub(crate) link: HostLink,
    pub(crate) host: Host,
    pub(crate) cli: PathBuf,
    pub(crate) pairing: Pairing,
    pub(crate) placement: CloudPlacement,
    pub(crate) claude_token: Option<ClaudeOauthToken>,
}

impl CloudSite {
    pub(crate) fn new(ready: ReadyRunner, host: Host, placement: CloudPlacement) -> Self {
        Self {
            link: ready.link,
            host,
            cli: ready.cli,
            pairing: ready.pairing,
            placement,
            claude_token: ready.claude_token,
        }
    }

    pub(crate) fn run(&self, program: String, start: &ProcessStart) -> CloudRun {
        CloudRun::new(
            self.link.clone(),
            self.cli.clone(),
            self.pairing.clone(),
            program,
            start.id.clone(),
            start.from,
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) enum NodeSite {
    Env,
    Cloud(Box<CloudSite>),
    Waiting(CloudRunnerId),
}

pub(crate) fn placement_of(environments: &Environments, node: &AgentRef) -> Option<CloudPlacement> {
    let data = environments.data(&node.environment)?;
    data.session(data.session_of(node.terminal)?)?.cloud.clone()
}

pub(crate) fn saved_process(environments: &Environments, node: &AgentRef) -> Option<CloudProcess> {
    environments
        .data(&node.environment)?
        .nodes
        .iter()
        .find(|kept| kept.id == node.terminal)?
        .cloud
        .clone()
}

pub(crate) fn site_of(placement: Option<CloudPlacement>, runners: &Runners) -> NodeSite {
    let Some(placement) = placement else {
        return NodeSite::Env;
    };
    let runner = placement.runner;
    let ready = runners
        .state(runner)
        .is_ready()
        .then(|| runners.directory.get(runner))
        .flatten();
    match (ready, runners.host(runner)) {
        (Some(ready), Some(host)) => {
            NodeSite::Cloud(Box::new(CloudSite::new(ready, host.clone(), placement)))
        }
        _ => NodeSite::Waiting(runner),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProcessStart {
    pub(crate) id: CloudProcessId,
    pub(crate) from: JournalOffset,
    pub(crate) fresh: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StartWait {
    Waiting,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProcessWrite {
    Started(AgentRef, CloudProcess),
    Offset(AgentRef, CloudProcessId, JournalOffset),
}

#[derive(Debug, Default)]
pub(crate) struct CloudRuns {
    attached: HashMap<AgentRef, CloudRun>,
    reattached: HashSet<AgentRef>,
    rewound: HashSet<AgentRef>,
    liveness: HashMap<AgentRef, Liveness>,
    wanted: Vec<CloudRunnerId>,
    writes: Vec<ProcessWrite>,
    pending: HashMap<AgentRef, CloudProcess>,
    offsets_at: Option<Instant>,
}

impl CloudRuns {
    pub(crate) fn want(&mut self, runner: CloudRunnerId) {
        if !self.wanted.contains(&runner) {
            self.wanted.push(runner);
        }
    }

    pub(crate) fn take_wanted(&mut self) -> Vec<CloudRunnerId> {
        std::mem::take(&mut self.wanted)
    }

    pub(crate) fn take_writes(&mut self) -> Vec<ProcessWrite> {
        self.pending.clear();
        std::mem::take(&mut self.writes)
    }

    pub(crate) fn offsets_due(&mut self, now: Instant, every: Duration) -> bool {
        if self.attached.is_empty() || self.offsets_at.is_some_and(|due| now < due) {
            return false;
        }
        self.offsets_at = Some(now + every);
        true
    }

    fn keep_consumed(&mut self, node: &AgentRef, saved: &mut Option<CloudProcess>) {
        let Some(run) = self.attached.remove(node) else {
            return;
        };
        let consumed = run.consumed();
        if let Some(kept) = saved.as_mut().filter(|kept| kept.id == run.process) {
            kept.offset = consumed;
            self.writes
                .push(ProcessWrite::Offset(node.clone(), run.process, consumed));
        }
    }

    fn fresh(&mut self, node: &AgentRef, site: &CloudSite) -> Result<ProcessStart, StartWait> {
        let id = node_process_id(&site.placement, node.terminal, &new_nonce())
            .ok_or_else(|| StartWait::Failed(NO_PROCESS_ID.to_owned()))?;
        let started = CloudProcess {
            id: id.clone(),
            offset: JournalOffset::default(),
        };
        self.pending.insert(node.clone(), started.clone());
        self.writes
            .push(ProcessWrite::Started(node.clone(), started));
        Ok(ProcessStart {
            id,
            from: JournalOffset::default(),
            fresh: true,
        })
    }

    pub(crate) fn start(
        &mut self,
        node: &AgentRef,
        site: &CloudSite,
        saved: Option<CloudProcess>,
        jobs: &Jobs,
    ) -> Result<ProcessStart, StartWait> {
        let mut saved = saved.or_else(|| self.pending.get(node).cloned());
        self.keep_consumed(node, &mut saved);
        let Some(saved) = saved else {
            return self.fresh(node, site);
        };
        match self.liveness.get(node).copied() {
            None => {
                self.liveness.insert(node.clone(), Liveness::Checking);
                check(node.clone(), site, saved.id, jobs);
                Err(StartWait::Waiting)
            }
            Some(Liveness::Checking) => Err(StartWait::Waiting),
            Some(Liveness::Running) => {
                self.liveness.remove(node);
                let from = if self.rewound.remove(node) {
                    JournalOffset::default()
                } else {
                    saved.offset
                };
                Ok(ProcessStart {
                    id: saved.id,
                    from,
                    fresh: false,
                })
            }
            Some(Liveness::Gone) => {
                self.liveness.remove(node);
                self.fresh(node, site)
            }
        }
    }

    pub(crate) fn checked(&mut self, checked: LivenessChecked) -> bool {
        let Some(known) = self.liveness.get_mut(&checked.node) else {
            return false;
        };
        *known = checked.liveness;
        true
    }

    pub(crate) fn attach(&mut self, node: AgentRef, run: CloudRun) {
        self.attached.insert(node, run);
    }

    pub(crate) fn rewind(&mut self, node: &AgentRef) -> bool {
        if !self.reattached.insert(node.clone()) {
            return false;
        }
        let Some(run) = self.attached.remove(node) else {
            return false;
        };
        self.writes.push(ProcessWrite::Offset(
            node.clone(),
            run.process,
            JournalOffset::default(),
        ));
        self.rewound.insert(node.clone());
        self.liveness.insert(node.clone(), Liveness::Running);
        true
    }

    pub(crate) fn forget(&mut self, node: &AgentRef) {
        if let Some(run) = self.attached.remove(node) {
            let consumed = run.consumed();
            self.writes
                .push(ProcessWrite::Offset(node.clone(), run.process, consumed));
        }
        self.reattached.remove(node);
        self.rewound.remove(node);
        self.liveness.remove(node);
    }

    pub(crate) fn consumed(&self) -> impl Iterator<Item = ProcessWrite> + '_ {
        self.attached.iter().map(|(node, run)| {
            ProcessWrite::Offset(node.clone(), run.process.clone(), run.consumed())
        })
    }
}

#[cfg(test)]
mod tests;
