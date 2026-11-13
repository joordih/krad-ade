use krad_chat::{opencode_models, ModelOffer, OpencodeServer, Runner};
use krad_domain::Host;
use krad_protocol::{EventKind, OpencodeShown, OpencodeSnapshot, OpencodeStatus};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, PoisonError, RwLock};

use crate::input::Input;
use crate::jobs::{JobDone, Jobs};

const JOB_FAILED: &str = "the opencode server could not start";

#[derive(Debug)]
pub(crate) enum OpencodeInput {
    Start {
        host: Host,
        runner: Runner,
        cwd: PathBuf,
    },
    Refresh(Host),
    CatalogChanged(Host),
    Forget(Host),
}

#[derive(Debug)]
pub(crate) struct OpencodeStarted {
    host: Host,
    started: Result<(Arc<OpencodeServer>, Vec<ModelOffer>), String>,
}

#[derive(Debug)]
pub(crate) struct OpencodeListed {
    host: Host,
    pub(crate) listed: Result<Vec<ModelOffer>, String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct OpencodeDirectory(Arc<RwLock<HashMap<Host, Arc<OpencodeServer>>>>);

impl OpencodeDirectory {
    pub(crate) fn server(&self, host: &Host) -> Option<Arc<OpencodeServer>> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(host)
            .cloned()
    }

    fn put(&self, host: Host, server: Option<Arc<OpencodeServer>>) {
        let mut servers = self.0.write().unwrap_or_else(PoisonError::into_inner);
        match server {
            Some(server) => servers.insert(host, server),
            None => servers.remove(&host),
        };
    }
}

#[derive(Debug)]
enum Served {
    Starting,
    Ready {
        server: Arc<OpencodeServer>,
        refresh: Refresh,
    },
    Failed(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Refresh {
    #[default]
    Idle,
    Running,
    Queued,
}

impl From<&Served> for OpencodeStatus {
    fn from(served: &Served) -> Self {
        match served {
            Served::Starting => Self::Starting,
            Served::Ready { .. } => Self::Ready,
            Served::Failed(error) => Self::Failed {
                error: error.clone(),
            },
        }
    }
}

pub(crate) enum Reach {
    Ready(Arc<OpencodeServer>),
    Waiting,
    Failed(String),
}

pub(crate) struct ModelsHeard {
    pub(crate) host: Host,
    pub(crate) models: Vec<ModelOffer>,
}

#[derive(Debug)]
pub(crate) struct OpencodeServers {
    hosts: HashMap<Host, Served>,
    directory: OpencodeDirectory,
    inbox: SyncSender<Input>,
}

fn shown(host: &Host, status: OpencodeStatus) -> EventKind {
    EventKind::OpencodeServer(OpencodeShown {
        host: host.clone(),
        status,
    })
}

impl OpencodeServers {
    pub(crate) fn new(inbox: SyncSender<Input>, directory: OpencodeDirectory) -> Self {
        Self {
            hosts: HashMap::new(),
            directory,
            inbox,
        }
    }

    pub(crate) fn reach(
        &mut self,
        host: &Host,
        (runner, cwd): (&Runner, &Path),
        jobs: &Jobs,
        outbox: &mut Vec<EventKind>,
    ) -> Reach {
        match self.hosts.get(host) {
            Some(Served::Ready { server, .. }) => Reach::Ready(Arc::clone(server)),
            Some(Served::Failed(error)) => Reach::Failed(error.clone()),
            Some(Served::Starting) => Reach::Waiting,
            None => {
                self.start(
                    host.clone(),
                    runner.clone(),
                    cwd.to_path_buf(),
                    jobs,
                    outbox,
                );
                Reach::Waiting
            }
        }
    }

    pub(crate) fn apply(&mut self, input: OpencodeInput, jobs: &Jobs, outbox: &mut Vec<EventKind>) {
        match input {
            OpencodeInput::Start { host, runner, cwd } => {
                if !self.hosts.contains_key(&host) {
                    self.start(host, runner, cwd, jobs, outbox);
                }
            }
            OpencodeInput::Refresh(host) | OpencodeInput::CatalogChanged(host) => {
                self.refresh(&host, jobs);
            }
            OpencodeInput::Forget(host) => {
                if self.hosts.remove(&host).is_some() {
                    self.directory.put(host.clone(), None);
                    outbox.push(shown(&host, OpencodeStatus::Stopped));
                }
            }
        }
    }

    #[tracing::instrument(skip_all, fields(host = %host))]
    fn start(
        &mut self,
        host: Host,
        runner: Runner,
        cwd: PathBuf,
        jobs: &Jobs,
        outbox: &mut Vec<EventKind>,
    ) {
        let job_host = host.clone();
        let started = jobs.run(move || {
            let started = OpencodeServer::start(&runner, &cwd, &[])
                .and_then(|server| Ok((Arc::clone(&server), opencode_models(&server, None)?)))
                .map_err(|error| error.to_string());
            JobDone::OpencodeStarted(OpencodeStarted {
                host: job_host,
                started,
            })
        });
        let served = match started {
            Ok(()) => Served::Starting,
            Err(error) => Served::Failed(format!("{JOB_FAILED}: {error}")),
        };
        outbox.push(shown(&host, OpencodeStatus::from(&served)));
        self.hosts.insert(host, served);
    }

    pub(crate) fn started(
        &mut self,
        done: OpencodeStarted,
        outbox: &mut Vec<EventKind>,
    ) -> Option<ModelsHeard> {
        let OpencodeStarted { host, started } = done;
        if !matches!(self.hosts.get(&host), Some(Served::Starting)) {
            return None;
        }
        let (served, heard) = match started {
            Ok((server, models)) => {
                self.watch_catalog(&host, &server);
                self.directory.put(host.clone(), Some(Arc::clone(&server)));
                tracing::info!(host = %host, "opencode server ready");
                let heard = ModelsHeard {
                    host: host.clone(),
                    models,
                };
                let ready = Served::Ready {
                    server,
                    refresh: Refresh::Idle,
                };
                (ready, Some(heard))
            }
            Err(error) => {
                tracing::debug!(host = %host, %error, "the opencode server did not start");
                (Served::Failed(error), None)
            }
        };
        outbox.push(shown(&host, OpencodeStatus::from(&served)));
        self.hosts.insert(host, served);
        heard
    }

    fn watch_catalog(&self, host: &Host, server: &OpencodeServer) {
        let inbox = self.inbox.clone();
        let host = host.clone();
        server.watch_catalog(move || {
            let changed = Input::Opencode(OpencodeInput::CatalogChanged(host.clone()));
            if let Err(TrySendError::Disconnected(_)) = inbox.try_send(changed) {
                tracing::trace!("the opencode catalog changed after the runtime stopped");
            }
        });
    }

    fn refresh(&mut self, host: &Host, jobs: &Jobs) {
        let Some(Served::Ready { server, refresh }) = self.hosts.get_mut(host) else {
            return;
        };
        if *refresh != Refresh::Idle {
            *refresh = Refresh::Queued;
            return;
        }
        let server = Arc::clone(server);
        let job_host = host.clone();
        let started = jobs.run(move || {
            JobDone::OpencodeListed(OpencodeListed {
                host: job_host,
                listed: opencode_models(&server, None).map_err(|error| error.to_string()),
            })
        });
        match started {
            Ok(()) => *refresh = Refresh::Running,
            Err(error) => tracing::warn!(%error, "the opencode model refresh could not start"),
        }
    }

    pub(crate) fn listed(&mut self, done: &OpencodeListed, jobs: &Jobs) -> Option<Host> {
        let Some(Served::Ready { refresh, .. }) = self.hosts.get_mut(&done.host) else {
            return None;
        };
        let queued = *refresh == Refresh::Queued;
        *refresh = Refresh::Idle;
        if queued {
            self.refresh(&done.host, jobs);
        }
        Some(done.host.clone())
    }

    pub(crate) fn snapshot(&self) -> OpencodeSnapshot {
        let mut servers: Vec<OpencodeShown> = self
            .hosts
            .iter()
            .map(|(host, served)| OpencodeShown {
                host: host.clone(),
                status: OpencodeStatus::from(served),
            })
            .collect();
        servers.sort_by_key(|shown| shown.host.to_string());
        OpencodeSnapshot { servers }
    }
}
