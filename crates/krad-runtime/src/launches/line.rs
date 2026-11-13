use krad_agents::CLI_PROGRAM;
use krad_domain::{
    code_tools_launch_args, hook_launch_args, launch_with_prompt, quote_for_shell, resume_line,
    AgentPreset, LaunchIntent, ModelDelivery, Settings, ShellQuoting,
};
use krad_protocol::RemoteBusInfo;
use krad_session::{clear_screen_prefix, SHELL_QUOTING};

use crate::bus::BusAccess;

const CLI_WSL_NAME: &str = "krad-ade-agents.exe";

pub const BUS_HINT: &str = "You run inside Krad ADE next to other coding agents. Run 'krad-ade-agents help' to see them, message them and wait for them.";
pub const LEAD_HINT: &str = "You lead this Krad ADE session: you split the work and start and direct the other coding agents of its group. Run 'krad-ade-agents help lead' first to learn how, and follow it.";

#[must_use]
pub const fn cli_name(in_wsl: bool) -> &'static str {
    if in_wsl {
        CLI_WSL_NAME
    } else {
        CLI_PROGRAM
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentRole {
    Member,
    Lead,
}

impl AgentRole {
    #[must_use]
    pub const fn hint(self) -> &'static str {
        match self {
            Self::Member => BUS_HINT,
            Self::Lead => LEAD_HINT,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LaunchSite {
    Native,
    Wsl,
    Ssh,
}

impl LaunchSite {
    pub(crate) const fn quoting(self) -> ShellQuoting {
        match self {
            Self::Native => SHELL_QUOTING,
            Self::Wsl | Self::Ssh => ShellQuoting::Posix,
        }
    }

    pub(crate) const fn clear_prefix(self) -> &'static str {
        clear_screen_prefix(!matches!(self, Self::Native))
    }

    pub(crate) const fn is_wsl(self) -> bool {
        matches!(self, Self::Wsl)
    }
}

pub(crate) struct LineContext<'a> {
    pub(crate) settings: &'a Settings,
    pub(crate) site: LaunchSite,
    pub(crate) bus: Option<&'a BusAccess>,
    pub(crate) remote: Option<&'a RemoteBusInfo>,
    pub(crate) role: AgentRole,
    pub(crate) tools_offered: bool,
}

pub(crate) type AgentLine = (String, Option<String>);

impl LineContext<'_> {
    pub(crate) fn line(&self, intent: &LaunchIntent) -> Option<AgentLine> {
        let quoting = self.site.quoting();
        match intent {
            LaunchIntent::Preset { preset } => self.agent_line(*preset, "", None),
            LaunchIntent::Agent {
                preset,
                prompt,
                model,
            } => self.agent_line(
                *preset,
                prompt,
                model.as_ref().map(krad_domain::ModelId::as_str),
            ),
            LaunchIntent::Resume { preset, resume } => {
                self.agent_line(*preset, "", None)
                    .map(|(line, after_start)| match resume {
                        Some(id) => (resume_line(&line, *preset, id, quoting), after_start),
                        None => (line, after_start),
                    })
            }
            LaunchIntent::Task { task } => Some((task.command_line(quoting), None)),
        }
    }

    fn agent_line(
        &self,
        preset: AgentPreset,
        prompt: &str,
        model: Option<&str>,
    ) -> Option<AgentLine> {
        let mut line = self.settings.launch_line(preset)?;
        let quoting = self.site.quoting();
        if let (Some(model), Some(ModelDelivery::Flag(flag))) = (model, preset.model_delivery()) {
            line = format!("{line} {flag} {}", quote_for_shell(model, quoting));
        }
        let cli = cli_name(self.site.is_wsl());
        let remote = self.remote.filter(|_| self.site == LaunchSite::Ssh);
        let reaches = self.site != LaunchSite::Ssh || remote.is_some();
        if let Some(bus) = self.bus.filter(|_| reaches) {
            let hook_settings = match remote {
                Some(remote) => Some(remote.claude_settings.to_string_lossy().into_owned()),
                None => bus
                    .claude_settings
                    .as_ref()
                    .filter(|_| self.site == LaunchSite::Native)
                    .map(|path| path.to_string_lossy().into_owned()),
            };
            if let Some(args) = hook_launch_args(preset, cli, quoting, hook_settings.as_deref()) {
                line = format!("{line} {args}");
            }
            if self.tools_offered {
                if let Some(args) = code_tools_launch_args(preset, cli, quoting) {
                    line = format!("{line} {args}");
                }
            }
            if let Some(flag) = preset.system_prompt_flag() {
                let hint = self.role.hint().replace(CLI_PROGRAM, cli);
                line = format!("{line} {flag} {}", quote_for_shell(&hint, quoting));
            }
        }
        Some(launch_with_prompt(&line, preset, prompt, quoting))
    }
}
