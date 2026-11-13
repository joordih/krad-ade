use krad_chat::{start_chat, AcpConfig, ChatBackend, EngineConfig, LiveChat, LiveConfig, Runner};
use krad_domain::{AgentPreset, Effort, GatewayModel, ModelId, PermissionMode};
use krad_host::HostLink;
use std::path::PathBuf;
use std::sync::Arc;

use super::chat::Chat;
use super::queue::ChatQueue;
use super::ChatWork;
use crate::gateway::GatewayReached;
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
                    ChatBackend::Gateway(reached.access.clone(), served(*config, reached.models))
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

fn served(mut config: EngineConfig, models: &[GatewayModel]) -> EngineConfig {
    if config.models.is_empty() {
        config.models = models.to_vec();
    }
    if config.model.is_none() {
        config.model = models
            .first()
            .and_then(|model| ModelId::parse(&model.offer.value));
    }
    config
}

pub(crate) fn reach_opencode(config: &LiveConfig, work: &mut ChatWork<'_>) -> Reach {
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
    OpencodeOn(Box<LiveConfig>),
    Opencode(Arc<krad_chat::OpencodeServer>),
    Acp(PathBuf),
    Unread,
}

impl HistorySource {
    pub(super) fn of(chat: &Chat) -> Self {
        let config = match &chat.launch {
            ChatLaunch::Gateway(config) => &**config,
            ChatLaunch::Opencode(config) => return Self::OpencodeOn(config.clone()),
            ChatLaunch::Acp(_) => {
                return krad_store::acp_transcript_file(&chat.id.environment, chat.id.terminal)
                    .map_or(Self::Unread, Self::Acp);
            }
            ChatLaunch::Backend(backend) => match &**backend {
                ChatBackend::Claude(_) => {
                    return Self::Claude {
                        link: chat.place.link.clone(),
                        root: chat.place.root.clone(),
                    }
                }
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
}

pub(super) fn ensure_live(
    chat: &mut Chat,
    queue: &ChatQueue,
    generation: u64,
    work: &mut ChatWork<'_>,
) -> bool {
    if chat.live().is_some() {
        return true;
    }
    let gateway = work.gateway;
    let launch = match (
        chosen(chat.launch.clone(), &chat.choices),
        chat.acp.as_mut(),
    ) {
        (ChatLaunch::Acp(config), Some(acp)) => {
            match acp.prepared(*config, (&chat.id, chat.opened), work.jobs) {
                Ok(backend) => ChatLaunch::from(backend),
                Err(Unready::Waiting) => return false,
                Err(Unready::Problem(problem)) => {
                    chat.problem = Some(problem);
                    return false;
                }
            }
        }
        (launch, _) => launch,
    };
    let backend = match launch.resolve(gateway, |config| reach_opencode(config, work)) {
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
mod tests {
    use super::*;
    use krad_chat::BusVars;

    fn waiting(_: &LiveConfig) -> Reach {
        Reach::Waiting
    }

    fn codex(model: Option<&str>) -> ChatLaunch {
        ChatLaunch::from(ChatBackend::Codex(LiveConfig {
            runner: Runner::Native(PathBuf::from("codex")),
            model: model.and_then(ModelId::parse),
            cwd: PathBuf::from("/work"),
            system_prompt: None,
            effort: None,
            permission_mode: None,
            bus_vars: BusVars::default(),
            usage_budget: std::sync::Arc::default(),
        }))
    }

    #[test]
    fn a_restarted_chat_starts_with_the_choices_made_since_it_opened() {
        let template = codex(Some("gpt-5"));
        let mut choices = Choices::from(&template);
        assert_eq!(choices.model, ModelId::parse("gpt-5"));
        choices.model = ModelId::parse("gpt-5-codex");
        choices.effort = Some(Effort::High);
        let Ok(ChatBackend::Codex(config)) = chosen(template, &choices).resolve(None, waiting)
        else {
            unreachable!("the harness does not change");
        };
        assert_eq!(config.model, ModelId::parse("gpt-5-codex"));
        assert_eq!(config.effort, Some(Effort::High));
    }

    #[test]
    fn a_gateway_launch_takes_the_runtime_access_and_served_models_and_waits_without_them() {
        let Ok(ChatBackend::Codex(config)) = codex(None).resolve(None, waiting) else {
            unreachable!("the template is a Codex chat");
        };
        let engine = EngineConfig {
            host: HostLink::Local,
            sessions: PathBuf::from("/sessions"),
            home: None,
            cwd: config.cwd,
            model: None,
            models: Vec::new(),
            context_windows: std::collections::HashMap::new(),
            system_prompt: None,
            effort: None,
            permission_mode: None,
            bus_vars: BusVars::default(),
            mcp: Vec::new(),
            skills: krad_domain::SkillSettings::default(),
        };
        let launch = ChatLaunch::Gateway(Box::new(engine.clone()));
        assert_eq!(
            launch.clone().resolve(None, waiting).err(),
            Some(Unready::Problem(GATEWAY_UNREACHED.to_owned()))
        );
        let (Ok(url), Some(key)) = (
            "http://127.0.0.1:1".parse(),
            krad_domain::GatewayKey::parse("sk-test"),
        ) else {
            unreachable!("a loopback address and a non-blank key");
        };
        let access = krad_domain::GatewayAccess { url, key };
        let served = [GatewayModel {
            offer: krad_domain::ModelOffer {
                value: "gemini-3-pro".to_owned(),
                ..krad_domain::ModelOffer::default()
            },
            vendor: krad_domain::Vendor::Google,
        }];
        let reached = GatewayReached {
            access: &access,
            models: &served,
        };
        let Ok(ChatBackend::Gateway(used, started)) = launch.resolve(Some(reached), waiting) else {
            unreachable!("a reached Gateway starts its chat");
        };
        assert_eq!(used, access);
        assert_eq!(started.models, served);
        assert_eq!(started.model, ModelId::parse("gemini-3-pro"));
        assert_eq!(started.cwd, engine.cwd);
    }

    #[test]
    fn only_harnesses_moved_to_the_runtime_run_there() {
        assert!(runs_in_runtime(AgentPreset::ClaudeCode));
        assert!(runs_in_runtime(AgentPreset::Codex));
        assert!(runs_in_runtime(AgentPreset::Gateway));
        assert!(runs_in_runtime(AgentPreset::Opencode));
        assert!(runs_in_runtime(AgentPreset::Acp(
            "fake"
                .parse()
                .unwrap_or_else(|_| unreachable!("a valid agent id"))
        )));
        assert!(!runs_in_runtime(AgentPreset::Shell));
    }
}
