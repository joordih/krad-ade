use krad_chat::{claude_models, codex_models, Runner};
use krad_domain::{AgentPreset, Host, ModelOffer};
use krad_protocol::{EventKind, ListedModels, ModelsCommand, ModelsOrigin, ModelsSnapshot, Reply};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::jobs::{JobDone, Jobs};

type ModelsKey = (Host, AgentPreset);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Listing {
    Claude,
    Codex,
}

impl Listing {
    const fn of(harness: AgentPreset) -> Option<Self> {
        match harness {
            AgentPreset::ClaudeCode => Some(Self::Claude),
            AgentPreset::Codex => Some(Self::Codex),
            AgentPreset::Shell
            | AgentPreset::Opencode
            | AgentPreset::Gateway
            | AgentPreset::Acp(_) => None,
        }
    }

    fn run(self, runner: &Runner, cwd: &Path) -> Result<Vec<ModelOffer>, String> {
        match self {
            Self::Claude => claude_models(runner, cwd).map_err(|error| error.to_string()),
            Self::Codex => codex_models(runner, cwd).map_err(|error| error.to_string()),
        }
    }
}

const fn keeps(harness: AgentPreset) -> bool {
    matches!(harness, AgentPreset::Acp(_) | AgentPreset::Opencode) || Listing::of(harness).is_some()
}

#[derive(Debug, Default)]
pub(crate) struct Models {
    listed: HashMap<ModelsKey, Vec<ModelOffer>>,
    asked: HashSet<ModelsKey>,
}

impl Models {
    pub(crate) fn offers(&self, host: &Host, harness: AgentPreset) -> &[ModelOffer] {
        self.listed
            .get(&(host.clone(), harness))
            .map_or(&[], Vec::as_slice)
    }

    pub(crate) fn execute(
        &mut self,
        command: &ModelsCommand,
        outbox: &mut Vec<EventKind>,
    ) -> Option<Reply> {
        match command {
            ModelsCommand::Keep {
                host,
                harness,
                models,
            } => {
                let key = (host.clone(), *harness);
                self.keep(key, models.clone(), ModelsOrigin::Chat, outbox);
                Some(Reply::Accepted)
            }
            ModelsCommand::List { .. } | ModelsCommand::Unknown => None,
        }
    }

    pub(crate) fn list(&mut self, key: ModelsKey, runner: Runner, cwd: PathBuf, jobs: &Jobs) {
        let Some(listing) = Listing::of(key.1) else {
            return;
        };
        if self.asked.contains(&key) {
            return;
        }
        let job_key = key.clone();
        let started = jobs.run(move || JobDone::ModelsListed {
            key: job_key,
            listed: listing.run(&runner, &cwd),
        });
        match started {
            Ok(()) => {
                self.asked.insert(key);
            }
            Err(error) => tracing::warn!(%error, "a model listing could not start"),
        }
    }

    pub(crate) fn listed(
        &mut self,
        key: ModelsKey,
        listed: Result<Vec<ModelOffer>, String>,
        outbox: &mut Vec<EventKind>,
    ) {
        match listed {
            Ok(models) => self.keep(key, models, ModelsOrigin::Listing, outbox),
            Err(error) => outbox.push(EventKind::ModelListFailed {
                host: key.0,
                harness: key.1,
                error,
            }),
        }
    }

    fn keep(
        &mut self,
        key: ModelsKey,
        models: Vec<ModelOffer>,
        origin: ModelsOrigin,
        outbox: &mut Vec<EventKind>,
    ) {
        if models.is_empty() || !keeps(key.1) {
            return;
        }
        self.listed.insert(key.clone(), models.clone());
        outbox.push(EventKind::ModelsListed {
            listed: ListedModels {
                host: key.0,
                harness: key.1,
                models,
            },
            origin,
        });
    }

    pub(crate) fn snapshot(&self) -> ModelsSnapshot {
        ModelsSnapshot {
            listed: self
                .listed
                .iter()
                .map(|((host, harness), models)| ListedModels {
                    host: host.clone(),
                    harness: *harness,
                    models: models.clone(),
                })
                .collect(),
        }
    }
}
