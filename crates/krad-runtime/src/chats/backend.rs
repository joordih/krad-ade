use krad_chat::{start_chat, AcpConfig, ChatBackend, EngineConfig, LiveChat, LiveConfig, Runner};
use krad_domain::{AgentPreset, Effort, ModelId, PermissionMode};
use krad_host::HostLink;
use krad_protocol::AgentRef;
use krad_store::gateway_sessions_dir;
use std::path::PathBuf;
use std::sync::Arc;

use super::brief::ChatBrief;
use super::chat::Chat;
use super::mcp_prep::{ready_mcp, McpPrep};
use super::queue::ChatQueue;
use super::resolve::{chat_folder, resolve_acp, LaunchFacts};
use super::ChatWork;
use crate::gateway::GatewayReached;
use crate::launches::AgentRole;
use crate::mcp::McpLaunch;
use crate::opencode::Reach;

#[must_use]
pub const fn runs_in_runtime(harness: AgentPreset) -> bool {
    matches!(
        harness,
        AgentPreset::ClaudeCode
            | AgentPreset::Codex
            | AgentPreset::Gateway
            | AgentPreset::Opencode
            | AgentPreset::Acp(_)
    )
}

const GATEWAY_UNREACHED: &str =
    "The Gateway is not reachable yet: check its address and key in Settings › Gateway";
pub(crate) const OPENCODE_STARTING: &str = "opencode is still starting";
const ACP_UNPREPARED: &str = "an ACP agent starts only inside an open chat";
const UNKNOWN_PLACE: &str = "the chat's environment is not open in the runtime";

pub type LiveStarted = Result<LiveChat, String>;

#[derive(Clone)]
pub enum ChatLaunch {
    Backend(Box<ChatBackend>),
    Gateway(Box<EngineConfig>),
    Opencode(Box<LiveConfig>),
    Acp(Box<AcpConfig>),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Unready {
    Waiting,
    Problem(String),
}

impl From<Unready> for String {
    fn from(unready: Unready) -> Self {
        match unready {
            Unready::Waiting => OPENCODE_STARTING.to_owned(),
            Unready::Problem(problem) => problem,
        }
    }
}

impl ChatLaunch {
    /// # Errors
    ///
    /// Fails when a Gateway chat starts before the runtime reached the Gateway, or an opencode
    /// chat before its host's server is ready.
    pub(crate) fn resolve(
        self,
        gateway: Option<GatewayReached<'_>>,
        opencode: impl FnOnce(&LiveConfig) -> Reach,
    ) -> Result<ChatBackend, Unready> {
        match self {
            Self::Backend(backend) => Ok(*backend),
            Self::Gateway(config) => gateway
                .map(|reached| {
                    ChatBackend::Gateway(reached.access.clone(), served(*config, reached))
                })
                .ok_or_else(|| Unready::Problem(GATEWAY_UNREACHED.to_owned())),
            Self::Opencode(config) => match opencode(&config) {
                Reach::Ready(server) => Ok(ChatBackend::Opencode(server, *config)),
                Reach::Waiting => Err(Unready::Waiting),
                Reach::Failed(error) => Err(Unready::Problem(error)),
            },
            Self::Acp(_) => Err(Unready::Problem(ACP_UNPREPARED.to_owned())),
        }
    }
}

fn served(mut config: EngineConfig, reached: GatewayReached<'_>) -> EngineConfig {
    let GatewayReached {
        models, settings, ..
    } = reached;
    config.models = models.to_vec();
    config.context_windows.clone_from(&settings.context_windows);
    if config.model.is_none() {
        let offered = settings
            .models
            .iter()
            .find(|kept| {
                models
                    .iter()
                    .any(|model| model.offer.value == kept.as_str())
            })
            .cloned();
        config.model = offered.or_else(|| {
            models
                .first()
                .and_then(|model| ModelId::parse(&model.offer.value))
        });
    }
    config
}

pub(super) fn reach_opencode(config: &LiveConfig, work: &mut ChatWork<'_>) -> Reach {
    work.opencode.reach(
        &config.runner.link().host(),
        (&config.runner, &config.cwd),
        work.jobs,
        work.outbox,
    )
}

impl From<ChatBackend> for ChatLaunch {
    fn from(backend: ChatBackend) -> Self {
        Self::Backend(Box::new(backend))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Choices {
    pub(super) model: Option<ModelId>,
    pub(super) effort: Option<Effort>,
    pub(super) permission_mode: Option<PermissionMode>,
}

impl From<&ChatLaunch> for Choices {
    fn from(launch: &ChatLaunch) -> Self {
        let config = match launch {
            ChatLaunch::Gateway(config) => config,
            ChatLaunch::Opencode(config) => {
                return Self {
                    model: config.model.clone(),
                    effort: config.effort,
                    permission_mode: config.permission_mode,
                };
            }
            ChatLaunch::Acp(_) => return Self::default(),
            ChatLaunch::Backend(backend) => match &**backend {
                ChatBackend::Claude(config)
                | ChatBackend::Codex(config)
                | ChatBackend::Opencode(_, config) => {
                    return Self {
                        model: config.model.clone(),
                        effort: config.effort,
                        permission_mode: config.permission_mode,
                    };
                }
                ChatBackend::Gateway(_, config) => config,
                ChatBackend::Acp(_) => return Self::default(),
            },
        };
        Self {
            model: config.model.clone(),
            effort: config.effort,
            permission_mode: config.permission_mode,
        }
    }
}

fn choose_engine(config: &mut EngineConfig, choices: &Choices) {
    if choices.model.is_some() {
        config.model.clone_from(&choices.model);
    }
    config.effort = choices.effort;
    config.permission_mode = choices.permission_mode;
}

fn choose_live(config: &mut LiveConfig, choices: &Choices) {
    config.model.clone_from(&choices.model);
    config.effort = choices.effort;
    config.permission_mode = choices.permission_mode;
}

fn chosen(mut launch: ChatLaunch, choices: &Choices) -> ChatLaunch {
    let backend = match &mut launch {
        ChatLaunch::Gateway(config) => {
            choose_engine(config, choices);
            return launch;
        }
        ChatLaunch::Opencode(config) => {
            choose_live(config, choices);
            return launch;
        }
        ChatLaunch::Acp(_) => return launch,
        ChatLaunch::Backend(backend) => &mut **backend,
    };
    match backend {
        ChatBackend::Claude(config)
        | ChatBackend::Codex(config)
        | ChatBackend::Opencode(_, config) => choose_live(config, choices),
        ChatBackend::Gateway(_, config) => choose_engine(config, choices),
        ChatBackend::Acp(_) => {}
    }
    launch
}

pub(super) enum HistorySource {
    Claude {
        link: HostLink,
        root: Option<PathBuf>,
    },
    Codex {
        runner: Runner,
        cwd: PathBuf,
    },
    Gateway {
        sessions: PathBuf,
        cwd: PathBuf,
    },
    OpencodeOn(Runner, PathBuf),
    Opencode(Arc<krad_chat::OpencodeServer>),
    Acp(PathBuf),
    Unread,
}

impl HistorySource {
    pub(super) fn of(chat: &Chat, facts: &LaunchFacts<'_>) -> Self {
        let Some(launch) = &chat.prepared else {
            return Self::briefed(chat, facts);
        };
        let config = match launch {
            ChatLaunch::Gateway(config) => &**config,
            ChatLaunch::Opencode(config) => {
                return Self::OpencodeOn(config.runner.clone(), config.cwd.clone())
            }
            ChatLaunch::Acp(_) => {
                return krad_store::acp_transcript_file(&chat.id.environment, chat.id.terminal)
                    .map_or(Self::Unread, Self::Acp);
            }
            ChatLaunch::Backend(backend) => match &**backend {
                ChatBackend::Claude(_) => return Self::claude(chat),
                ChatBackend::Codex(config) => {
                    return Self::Codex {
                        runner: config.runner.clone(),
                        cwd: config.cwd.clone(),
                    }
                }
                ChatBackend::Gateway(_, config) => config,
                ChatBackend::Opencode(server, _) => return Self::Opencode(Arc::clone(server)),
                ChatBackend::Acp(_) => return Self::Unread,
            },
        };
        Self::Gateway {
            sessions: config.sessions.clone(),
            cwd: config.cwd.clone(),
        }
    }

    fn claude(chat: &Chat) -> Self {
        Self::Claude {
            link: chat.place.link.clone(),
            root: chat.place.root.clone(),
        }
    }

    fn briefed(chat: &Chat, facts: &LaunchFacts<'_>) -> Self {
        let runner = || facts.history_runner(&chat.id, chat.harness, &chat.place);
        match chat.harness {
            AgentPreset::ClaudeCode => Self::claude(chat),
            AgentPreset::Codex => {
                runner().map_or(Self::Unread, |(runner, cwd)| Self::Codex { runner, cwd })
            }
            AgentPreset::Opencode => {
                runner().map_or(Self::Unread, |(runner, cwd)| Self::OpencodeOn(runner, cwd))
            }
            AgentPreset::Gateway => {
                gateway_sessions_dir().map_or(Self::Unread, |sessions| Self::Gateway {
                    sessions,
                    cwd: chat_folder(&chat.place),
                })
            }
            AgentPreset::Acp(_) => {
                krad_store::acp_transcript_file(&chat.id.environment, chat.id.terminal)
                    .map_or(Self::Unread, Self::Acp)
            }
            AgentPreset::Shell => Self::Unread,
        }
    }
}

fn planned(
    chat: &mut Chat,
    mcp: McpLaunch,
    work: &mut ChatWork<'_>,
) -> Result<ChatLaunch, Unready> {
    let Some(prepared) = &chat.prepared else {
        let resolved = match chat.harness {
            AgentPreset::Acp(agent) => resolve_acp(
                (&chat.id, agent, &chat.place),
                (&chat.brief, &chat.choices),
                mcp.starts,
                work,
            )?,
            harness => work.facts.resolve(
                (&chat.id, harness, &chat.place),
                (&chat.brief, &chat.choices),
                mcp.starts,
            )?,
        };
        if resolved.model_dropped {
            chat.choices.model = None;
        }
        chat.start_notices.extend(resolved.notices);
        chat.start_notices.extend(mcp.problems);
        return Ok(chosen(resolved.launch, &chat.choices));
    };
    let mut launch = chosen(prepared.clone(), &chat.choices);
    if let ChatLaunch::Acp(config) = &mut launch {
        config.mcp = mcp.starts;
        chat.start_notices.extend(mcp.problems);
    }
    Ok(launch)
}

fn changed_role(chat: &Chat, role: AgentRole) -> bool {
    chat.prepared.is_none()
        && chat.harness.acp_agent().is_none()
        && chat.started_as.is_some_and(|started| started != role)
}

pub(crate) fn resolve_once(
    (chat, harness): (&AgentRef, AgentPreset),
    brief: &ChatBrief,
    work: &mut ChatWork<'_>,
) -> Result<ChatBackend, Unready> {
    let place = work
        .facts
        .place(chat)
        .ok_or_else(|| Unready::Problem(UNKNOWN_PLACE.to_owned()))?;
    let resolved = work.facts.resolve(
        (chat, harness, &place),
        (brief, &Choices::from(brief)),
        Vec::new(),
    )?;
    let gateway = work.gateway;
    resolved
        .launch
        .resolve(gateway, |config| reach_opencode(config, work))
}

pub(super) fn ensure_live(
    chat: &mut Chat,
    queue: &ChatQueue,
    generation: u64,
    work: &mut ChatWork<'_>,
) -> bool {
    let role = work.facts.role(&chat.id);
    if chat.live().is_some() {
        if !changed_role(chat, role) {
            return true;
        }
        if let Some(live) = chat.live.take() {
            live.stop();
        }
    }
    let Some(mcp) = ready_mcp(chat, work) else {
        return false;
    };
    let gateway = work.gateway;
    let launch = match (planned(chat, mcp, work), chat.acp.as_mut()) {
        (Ok(ChatLaunch::Acp(config)), Some(acp)) => acp
            .prepared(
                *config,
                (&chat.id, chat.opened),
                (work.jobs, work.facts.secrets),
            )
            .map(ChatLaunch::from),
        (planned, _) => planned,
    };
    let backend =
        launch.and_then(|launch| launch.resolve(gateway, |config| reach_opencode(config, work)));
    let backend = match backend {
        Ok(backend) => backend,
        Err(Unready::Waiting) => return false,
        Err(Unready::Problem(problem)) => {
            chat.problem = Some(problem);
            return false;
        }
    };
    let sink = queue.sink(chat.id.clone(), generation);
    match start_chat(backend, &chat.session, sink) {
        Ok(live) => {
            chat.live = Some(live);
            chat.started_as = Some(role);
            chat.mcp = McpPrep::Unread;
            chat.generation = generation;
            chat.confirmed = false;
            true
        }
        Err(error) => {
            tracing::debug!(%error, "a chat could not start");
            chat.problem = Some(error.to_string());
            false
        }
    }
}

#[cfg(test)]
mod tests;
