use krad_chat::{
    install_managed_adapter, managed_adapter_program, managed_adapter_update_due, Runner,
};
use krad_domain::{
    app_kept_acp_package, parse_wsl_path, AcpAgent, AcpAgentId, AdapterFound, AdapterStart,
    AdapterWork, ManagedAdapter, NpmPackage,
};
use krad_host::HostLink;
use krad_protocol::{EventKind, KeptAdapter};
use krad_session::find_program;
use krad_store::acp_adapters_dir;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::jobs::{JobDone, Jobs};

const NPM_PROGRAM: &str = "npm";
const NO_NODE_TEXT: &str = "Node.js (with npm) is not installed";
const JOB_NOT_STARTED: &str = "the adapter's work could not start";

#[derive(Debug)]
pub(crate) struct AdapterJob {
    agent: AcpAgentId,
    package: NpmPackage,
    folder: PathBuf,
    program: String,
}

#[derive(Debug)]
pub(crate) enum AdapterDone {
    Checked {
        job: AdapterJob,
        found: AdapterFound,
    },
    Worked {
        agent: AcpAgentId,
        work: AdapterWork,
        outcome: Result<PathBuf, String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AdapterLaunch {
    Launch(PathBuf),
    Waiting,
    Installing,
    NoNode,
    Failed(String),
    NoConfigFolder,
}

#[derive(Debug, Default)]
pub(crate) struct AcpAdapters {
    kept: HashMap<AcpAgentId, ManagedAdapter>,
}

#[must_use]
pub fn kept_package(agent: AcpAgentId, link: &HostLink, root: Option<&Path>) -> Option<NpmPackage> {
    let here = link.is_local() && root.and_then(parse_wsl_path).is_none();
    app_kept_acp_package(agent).filter(|_| here)
}

fn found_adapter(job: &AdapterJob) -> AdapterFound {
    if let Some(program) = managed_adapter_program(&job.folder, job.package) {
        let update_due = managed_adapter_update_due(&job.folder);
        return AdapterFound::Installed {
            program,
            update_due,
        };
    }
    if let Some(program) = find_program(&job.program) {
        return AdapterFound::Installed {
            program,
            update_due: false,
        };
    }
    if find_program(NPM_PROGRAM).is_none() {
        return AdapterFound::NoNode;
    }
    AdapterFound::Missing
}

fn run_adapter_work(job: &AdapterJob) -> Result<PathBuf, String> {
    let npm = find_program(NPM_PROGRAM).ok_or_else(|| NO_NODE_TEXT.to_owned())?;
    install_managed_adapter(&Runner::Native(npm), &job.folder, job.package)
        .map_err(|error| error.to_string())
}

impl AcpAdapters {
    pub(crate) fn start(
        &mut self,
        agent: &AcpAgent,
        package: NpmPackage,
        jobs: &Jobs,
        outbox: &mut Vec<EventKind>,
    ) -> AdapterLaunch {
        let Some(folder) = acp_adapters_dir().map(|dir| dir.join(agent.id.as_str())) else {
            return AdapterLaunch::NoConfigFolder;
        };
        let adapter = self.kept.entry(agent.id).or_default();
        match adapter.start() {
            AdapterStart::Launch(program) => AdapterLaunch::Launch(program),
            AdapterStart::Wait if adapter.is_installing() => AdapterLaunch::Installing,
            AdapterStart::Wait => AdapterLaunch::Waiting,
            AdapterStart::NoNode => AdapterLaunch::NoNode,
            AdapterStart::Failed(reason) => AdapterLaunch::Failed(reason),
            AdapterStart::Check => {
                let job = AdapterJob {
                    agent: agent.id,
                    package,
                    folder,
                    program: agent.program.clone(),
                };
                let started = jobs.run(move || {
                    let found = found_adapter(&job);
                    JobDone::Adapter(AdapterDone::Checked { job, found })
                });
                if let Err(error) = started {
                    tracing::warn!(%error, "could not start checking an ACP adapter");
                    *adapter = ManagedAdapter::Failed(JOB_NOT_STARTED.to_owned());
                }
                self.tell(agent.id, outbox);
                AdapterLaunch::Waiting
            }
        }
    }

    pub(crate) fn state(&self, agent: AcpAgentId) -> Option<&ManagedAdapter> {
        self.kept.get(&agent)
    }

    pub(crate) fn check(&mut self, agent: &AcpAgent, jobs: &Jobs, outbox: &mut Vec<EventKind>) {
        let Some(package) = app_kept_acp_package(agent.id) else {
            return;
        };
        let unchecked = self
            .kept
            .get(&agent.id)
            .is_none_or(|adapter| *adapter == ManagedAdapter::Unchecked);
        if unchecked {
            self.start(agent, package, jobs, outbox);
        }
    }

    pub(crate) fn retry(&mut self, agent: AcpAgentId, outbox: &mut Vec<EventKind>) {
        let Some(adapter) = self.kept.get_mut(&agent) else {
            return;
        };
        adapter.retry();
        self.tell(agent, outbox);
    }

    pub(crate) fn done(&mut self, done: AdapterDone, jobs: &Jobs, outbox: &mut Vec<EventKind>) {
        match done {
            AdapterDone::Checked { job, found } => {
                let agent = job.agent;
                let work = self.kept.entry(agent).or_default().checked(found);
                if let Some(work) = work {
                    tracing::info!(agent = %agent, ?work, "keeping an ACP adapter in the app's folder");
                    let started = jobs.run(move || {
                        let outcome = run_adapter_work(&job);
                        JobDone::Adapter(AdapterDone::Worked {
                            agent: job.agent,
                            work,
                            outcome,
                        })
                    });
                    if let Err(error) = started {
                        tracing::warn!(%error, "could not start installing an ACP adapter");
                        self.kept
                            .entry(agent)
                            .or_default()
                            .finished(work, Err(JOB_NOT_STARTED.to_owned()));
                    }
                }
                self.tell(agent, outbox);
            }
            AdapterDone::Worked {
                agent,
                work,
                outcome,
            } => {
                if let Err(error) = &outcome {
                    tracing::warn!(agent = %agent, ?work, %error, "the ACP adapter was not installed");
                }
                if let Some(adapter) = self.kept.get_mut(&agent) {
                    adapter.finished(work, outcome);
                }
                self.tell(agent, outbox);
            }
        }
    }

    fn tell(&self, agent: AcpAgentId, outbox: &mut Vec<EventKind>) {
        let adapter = self.kept.get(&agent).cloned().unwrap_or_default();
        outbox.push(EventKind::AcpAdapterChanged(KeptAdapter { agent, adapter }));
    }

    pub(crate) fn snapshot(&self) -> Vec<KeptAdapter> {
        self.kept
            .iter()
            .map(|(agent, adapter)| KeptAdapter {
                agent: *agent,
                adapter: adapter.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_this_computer_s_own_folders_keep_an_adapter() -> Result<(), String> {
        let kept: AcpAgentId = "claude-code-acp".parse().map_err(|_| "no id")?;
        assert!(kept_package(kept, &HostLink::Local, Some(Path::new("/work/api"))).is_some());
        assert!(kept_package(kept, &HostLink::Local, None).is_some());
        assert!(kept_package(
            kept,
            &HostLink::Local,
            Some(Path::new(r"\\wsl$\Ubuntu\home\dev"))
        )
        .is_none());
        let custom: AcpAgentId = "fake".parse().map_err(|_| "no id")?;
        assert!(kept_package(custom, &HostLink::Local, None).is_none());
        Ok(())
    }
}
