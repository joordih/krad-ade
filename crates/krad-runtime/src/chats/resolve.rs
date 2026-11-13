use krad_agents::{Caller, TerminalHandle, CLI_PROGRAM};
use krad_chat::{BusVars, ChatBackend, EngineConfig, LiveConfig, Runner};
use krad_domain::{model_label, parse_wsl_path, AgentPreset, Settings, UsageBudget};
use krad_host::HostLink;
use krad_protocol::{AgentRef, RemoteBusInfo};
use krad_session::find_program;
use krad_store::gateway_sessions_dir;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::backend::{ChatLaunch, Choices, Unready};
use super::brief::{ChatBrief, ChatRole};
use super::{ChatPlace, EditorBuffers};
use crate::bus::BusAccess;
use crate::environments::Environments;
use crate::hosts::{remote_caller_vars, Hosts, Reach};
use crate::kraddies::Kraddies;
use crate::launches::{cli_name, local_caller_vars, AgentRole};
use crate::models::Models;

mod acp;

pub(super) use acp::resolve_acp;

const NO_PROGRAM_TEXT: &str = "is not on PATH";
const NO_CHAT_TEXT: &str = "has no chat";
const NOT_IN_WSL_TEXT: &str = "chats do not run in WSL folders yet";
const NO_CONFIG_FOLDER_TEXT: &str = "Krad ADE has no config folder to keep the chat in";
const UNAVAILABLE_MODEL_NOTICE: &str =
    "is no longer offered here, so this message uses the default model";
const PROMPT_SEPARATOR: &str = "\n\n";

pub(crate) struct LaunchFacts<'a> {
    pub(crate) settings: &'a Settings,
    pub(crate) environments: &'a Environments,
    pub(crate) hosts: &'a Hosts,
    pub(crate) models: &'a Models,
    pub(crate) bus: Option<&'a BusAccess>,
    pub(crate) usage_budget: &'a Arc<UsageBudget>,
    pub(crate) kraddies: &'a Kraddies,
    pub(crate) buffers: &'a EditorBuffers,
}

pub(super) struct Resolved {
    pub(super) launch: ChatLaunch,
    pub(super) notices: Vec<String>,
    pub(super) model_dropped: bool,
}

struct Site<'a> {
    remote: Option<&'a RemoteBusInfo>,
    role: AgentRole,
}

fn problem(name: impl std::fmt::Display, text: &str) -> Unready {
    Unready::Problem(format!("{name} {text}"))
}

fn joined_prompt(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (Some(first), Some(second)) => Some(format!("{first}{PROMPT_SEPARATOR}{second}")),
        (first, second) => first.or(second),
    }
}

fn caller(chat: &AgentRef) -> Caller {
    Caller::Terminal(TerminalHandle {
        environment: chat.environment.as_str().to_owned(),
        terminal: chat.terminal.as_u64(),
    })
}

fn local_home() -> Option<PathBuf> {
    std::env::home_dir()
}

pub(super) fn in_wsl(place: &ChatPlace) -> bool {
    place.link.is_local() && place.root.as_deref().and_then(parse_wsl_path).is_some()
}

pub(super) fn chat_folder(place: &ChatPlace) -> PathBuf {
    place
        .root
        .clone()
        .or_else(|| match &place.link {
            HostLink::Remote(ssh) => ssh.home().map(Path::to_path_buf),
            HostLink::Local => local_home(),
        })
        .unwrap_or_default()
}

impl LaunchFacts<'_> {
    pub(crate) fn place(&self, chat: &AgentRef) -> Option<ChatPlace> {
        let data = self.environments.data(&chat.environment)?;
        let host = data.host.clone();
        Some(ChatPlace {
            link: self.hosts.link(&host),
            root: data.group_of(chat.terminal).map(|group| group.root.clone()),
            host,
        })
    }

    pub(super) fn tools_offered(&self) -> bool {
        self.settings.code_tools == krad_domain::CodeTools::On
            || self.settings.extension_tools == krad_domain::ExtensionTools::On
            || self.kraddies.offers_memory_tools()
    }

    fn site(&self, chat: &AgentRef, place: &ChatPlace) -> Result<Site<'_>, Unready> {
        let remote = match self.hosts.reach(&chat.environment, &place.host) {
            Reach::Ready(remote) => remote,
            Reach::BusPreparing | Reach::Unreached => return Err(Unready::Waiting),
        };
        Ok(Site {
            remote,
            role: self.role(chat),
        })
    }

    pub(super) fn role(&self, chat: &AgentRef) -> AgentRole {
        let leads = self
            .environments
            .data(&chat.environment)
            .and_then(|data| data.session_led_by(chat.terminal))
            .is_some();
        if leads {
            AgentRole::Lead
        } else {
            AgentRole::Member
        }
    }

    pub(super) fn bus_vars(
        &self,
        chat: &AgentRef,
        place: &ChatPlace,
        in_wsl: bool,
    ) -> Vec<(String, String)> {
        let Some(bus) = self.bus else {
            return Vec::new();
        };
        if place.host.is_local() {
            return local_caller_vars(bus, &caller(chat), in_wsl);
        }
        let remote = match self.hosts.reach(&chat.environment, &place.host) {
            Reach::Ready(remote) => remote,
            Reach::BusPreparing | Reach::Unreached => None,
        };
        remote.map_or_else(Vec::new, |remote| remote_caller_vars(remote, &caller(chat)))
    }

    fn memory(&self, chat: &AgentRef) -> Option<String> {
        let data = self.environments.data(&chat.environment)?;
        self.kraddies.preamble(data, chat.terminal)
    }

    fn system_prompt(&self, role: AgentRole, in_wsl: bool, memory: Option<&str>) -> Option<String> {
        let hint = self
            .bus
            .map(|_| role.hint().replace(CLI_PROGRAM, cli_name(in_wsl)));
        joined_prompt(hint, memory.map(str::to_owned))
    }

    fn program(&self, harness: AgentPreset) -> Result<String, Unready> {
        self.settings
            .launch_line(harness)
            .and_then(|line| line.split_whitespace().next().map(str::to_owned))
            .ok_or_else(|| problem(self.settings.acp.harness_name(harness), NO_PROGRAM_TEXT))
    }

    fn runner(
        &self,
        chat: &AgentRef,
        harness: AgentPreset,
        place: &ChatPlace,
    ) -> Result<(Runner, PathBuf), Unready> {
        let program = self.program(harness)?;
        self.program_runner(chat, (harness, program), place)
    }

    fn program_runner(
        &self,
        chat: &AgentRef,
        (harness, program): (AgentPreset, String),
        place: &ChatPlace,
    ) -> Result<(Runner, PathBuf), Unready> {
        let name = self.settings.acp.harness_name(harness);
        if let HostLink::Remote(ssh) = &place.link {
            let found = self
                .hosts
                .agents(&chat.environment)
                .ok_or(Unready::Waiting)?;
            if !found.contains(&harness) {
                return Err(problem(name, NO_PROGRAM_TEXT));
            }
            let cwd = place
                .root
                .clone()
                .or_else(|| ssh.home().map(Path::to_path_buf))
                .ok_or(Unready::Waiting)?;
            let link = place.link.clone();
            return Ok((Runner::Ssh { link, program }, cwd));
        }
        let wsl = place
            .root
            .as_deref()
            .and_then(|root| parse_wsl_path(root).map(|inside| (root, inside)));
        if let Some((root, inside)) = wsl {
            let runner = Runner::Wsl {
                distro: inside.distro,
                program,
            };
            return Ok((runner, root.to_path_buf()));
        }
        let runner = find_program(&program)
            .map(Runner::Native)
            .ok_or_else(|| problem(name, NO_PROGRAM_TEXT))?;
        let cwd = place.root.clone().or_else(local_home).unwrap_or_default();
        Ok((runner, cwd))
    }

    pub(super) fn history_runner(
        &self,
        chat: &AgentRef,
        harness: AgentPreset,
        place: &ChatPlace,
    ) -> Option<(Runner, PathBuf)> {
        self.runner(chat, harness, place).ok()
    }

    fn offered(&self, harness: AgentPreset, place: &ChatPlace, choices: &Choices) -> Offered {
        let Some(model) = choices.model.clone() else {
            return Offered::Kept(None);
        };
        let offers = self.models.offers(&place.host, harness);
        if offers.is_empty() || offers.iter().any(|offer| offer.answers_to(model.as_str())) {
            return Offered::Kept(Some(model));
        }
        Offered::Dropped(format!(
            "{} {UNAVAILABLE_MODEL_NOTICE}",
            model_label(model.as_str())
        ))
    }

    fn live_config(
        &self,
        chat: &AgentRef,
        (harness, place): (AgentPreset, &ChatPlace),
        brief: &ChatBrief,
        choices: &Choices,
    ) -> Result<(LiveConfig, Offered), Unready> {
        let site = self.site(chat, place)?;
        let (runner, cwd) = self.runner(chat, harness, place)?;
        let off_bus = runner.on_ssh() && site.remote.is_none();
        let (system_prompt, bus_vars) = match brief.role {
            ChatRole::Digest => (None, Vec::new()),
            ChatRole::Agent if off_bus => (self.memory(chat), Vec::new()),
            ChatRole::Agent => (
                self.system_prompt(site.role, runner.in_wsl(), self.memory(chat).as_deref()),
                self.bus_vars(chat, place, runner.in_wsl()),
            ),
        };
        let offered = self.offered(harness, place, choices);
        let config = LiveConfig {
            runner,
            model: offered.model(),
            cwd,
            system_prompt,
            effort: choices.effort,
            permission_mode: choices.permission_mode,
            bus_vars: BusVars(bus_vars),
            usage_budget: Arc::clone(self.usage_budget),
        };
        Ok((config, offered))
    }

    fn engine_config(
        &self,
        chat: &AgentRef,
        place: &ChatPlace,
        (brief, choices): (&ChatBrief, &Choices),
        mcp: Vec<krad_domain::McpServerStart>,
    ) -> Result<EngineConfig, Unready> {
        let site = self.site(chat, place)?;
        let sessions = gateway_sessions_dir()
            .ok_or_else(|| Unready::Problem(NO_CONFIG_FOLDER_TEXT.to_owned()))?;
        let home = match &place.link {
            HostLink::Local => local_home(),
            HostLink::Remote(ssh) => ssh.home().map(Path::to_path_buf),
        };
        let on_bus = place.host.is_local() || site.remote.is_some();
        let (system_prompt, bus_vars) = match brief.role {
            ChatRole::Digest => (None, Vec::new()),
            ChatRole::Agent if on_bus => (
                self.system_prompt(site.role, false, self.memory(chat).as_deref()),
                self.bus_vars(chat, place, false),
            ),
            ChatRole::Agent => (self.memory(chat), Vec::new()),
        };
        Ok(EngineConfig {
            sessions,
            home,
            cwd: chat_folder(place),
            model: choices.model.clone(),
            models: Vec::new(),
            context_windows: HashMap::new(),
            system_prompt,
            effort: choices.effort,
            permission_mode: choices.permission_mode,
            bus_vars: BusVars(bus_vars),
            mcp,
            host: place.link.clone(),
            skills: self.settings.skills.clone(),
        })
    }

    pub(super) fn resolve(
        &self,
        (chat, harness, place): (&AgentRef, AgentPreset, &ChatPlace),
        (brief, choices): (&ChatBrief, &Choices),
        mcp: Vec<krad_domain::McpServerStart>,
    ) -> Result<Resolved, Unready> {
        let name = self.settings.acp.harness_name(harness);
        let (config, offered) = match harness {
            AgentPreset::Gateway => {
                let config = self.engine_config(chat, place, (brief, choices), mcp)?;
                return Ok(Resolved {
                    launch: ChatLaunch::Gateway(Box::new(config)),
                    notices: Vec::new(),
                    model_dropped: false,
                });
            }
            AgentPreset::ClaudeCode | AgentPreset::Codex | AgentPreset::Opencode => {
                self.live_config(chat, (harness, place), brief, choices)?
            }
            AgentPreset::Acp(_) | AgentPreset::Shell => return Err(problem(name, NO_CHAT_TEXT)),
        };
        let model_dropped = matches!(offered, Offered::Dropped(_));
        let notices = offered.notice().into_iter().collect();
        let launch = match harness {
            AgentPreset::ClaudeCode => ChatLaunch::from(ChatBackend::Claude(config)),
            AgentPreset::Codex => ChatLaunch::from(ChatBackend::Codex(config)),
            _ if config.runner.in_wsl() => return Err(problem(name, NOT_IN_WSL_TEXT)),
            _ => ChatLaunch::Opencode(Box::new(config)),
        };
        Ok(Resolved {
            launch,
            notices,
            model_dropped,
        })
    }
}

enum Offered {
    Kept(Option<krad_domain::ModelId>),
    Dropped(String),
}

impl Offered {
    fn model(&self) -> Option<krad_domain::ModelId> {
        match self {
            Self::Kept(model) => model.clone(),
            Self::Dropped(_) => None,
        }
    }

    fn notice(self) -> Option<String> {
        match self {
            Self::Kept(_) => None,
            Self::Dropped(notice) => Some(notice),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hint_and_a_memory_preamble_join_with_a_blank_line() {
        assert_eq!(
            joined_prompt(Some("hint".to_owned()), Some("memory".to_owned())),
            Some("hint\n\nmemory".to_owned())
        );
        assert_eq!(
            joined_prompt(None, Some("memory".to_owned())),
            Some("memory".to_owned())
        );
        assert_eq!(joined_prompt(None, None), None);
    }
}
