use krad_chat::{start_chat, ChatBackend, EngineConfig, LiveChat, Runner};
use krad_domain::{AgentPreset, Effort, GatewayAccess, ModelId, PermissionMode};
use krad_host::HostLink;
use std::path::PathBuf;

use super::chat::Chat;
use super::queue::ChatQueue;

#[must_use]
pub const fn runs_in_runtime(harness: AgentPreset) -> bool {
    matches!(
        harness,
        AgentPreset::ClaudeCode | AgentPreset::Codex | AgentPreset::Gateway
    )
}

const GATEWAY_UNREACHED: &str =
    "The Gateway is not reachable yet: check its address and key in Settings › Gateway";

pub type LiveStarted = Result<LiveChat, String>;

#[derive(Clone)]
pub enum ChatLaunch {
    Backend(Box<ChatBackend>),
    Gateway(Box<EngineConfig>),
}

impl ChatLaunch {
    /// # Errors
    ///
    /// Fails when a Gateway chat starts before the runtime reached the Gateway.
    pub(crate) fn resolve(self, gateway: Option<&GatewayAccess>) -> Result<ChatBackend, String> {
        match self {
            Self::Backend(backend) => Ok(*backend),
            Self::Gateway(config) => gateway
                .map(|access| ChatBackend::Gateway(access.clone(), *config))
                .ok_or_else(|| GATEWAY_UNREACHED.to_owned()),
        }
    }
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

fn chosen(mut launch: ChatLaunch, choices: &Choices) -> ChatLaunch {
    let backend = match &mut launch {
        ChatLaunch::Gateway(config) => {
            choose_engine(config, choices);
            return launch;
        }
        ChatLaunch::Backend(backend) => &mut **backend,
    };
    match backend {
        ChatBackend::Claude(config)
        | ChatBackend::Codex(config)
        | ChatBackend::Opencode(_, config) => {
            config.model.clone_from(&choices.model);
            config.effort = choices.effort;
            config.permission_mode = choices.permission_mode;
        }
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
    Unread,
}

impl HistorySource {
    pub(super) fn of(chat: &Chat) -> Self {
        let config = match &chat.launch {
            ChatLaunch::Gateway(config) => &**config,
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
                ChatBackend::Opencode(..) | ChatBackend::Acp(_) => return Self::Unread,
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
    gateway: Option<&GatewayAccess>,
) -> bool {
    if chat.live().is_some() {
        return true;
    }
    let backend = match chosen(chat.launch.clone(), &chat.choices).resolve(gateway) {
        Ok(backend) => backend,
        Err(problem) => {
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
    use krad_chat::{BusVars, LiveConfig};

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
        let Ok(ChatBackend::Codex(config)) = chosen(template, &choices).resolve(None) else {
            unreachable!("the harness does not change");
        };
        assert_eq!(config.model, ModelId::parse("gpt-5-codex"));
        assert_eq!(config.effort, Some(Effort::High));
    }

    #[test]
    fn a_gateway_launch_takes_the_runtime_access_and_waits_without_it() {
        let Ok(ChatBackend::Codex(config)) = codex(None).resolve(None) else {
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
            launch.clone().resolve(None).err().as_deref(),
            Some(GATEWAY_UNREACHED)
        );
        let (Ok(url), Some(key)) = (
            "http://127.0.0.1:1".parse(),
            krad_domain::GatewayKey::parse("sk-test"),
        ) else {
            unreachable!("a loopback address and a non-blank key");
        };
        let access = GatewayAccess { url, key };
        let Ok(ChatBackend::Gateway(used, started)) = launch.resolve(Some(&access)) else {
            unreachable!("a reached Gateway starts its chat");
        };
        assert_eq!((used, started), (access, engine));
    }

    #[test]
    fn only_harnesses_moved_to_the_runtime_run_there() {
        assert!(runs_in_runtime(AgentPreset::ClaudeCode));
        assert!(runs_in_runtime(AgentPreset::Codex));
        assert!(runs_in_runtime(AgentPreset::Gateway));
        assert!(!runs_in_runtime(AgentPreset::Opencode));
        assert!(!runs_in_runtime(AgentPreset::Shell));
    }
}
