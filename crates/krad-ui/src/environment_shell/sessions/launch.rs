use gpui_kit::App;
use krad_domain::{
    parse_wsl_path, AgentPreset, EnvironmentName, LaunchIntent, ModelId, ProjectTask,
    RelaunchIntent, ResumeId, TerminalId, WslPath,
};
use std::path::Path;

use super::super::shell_site::ShellSite;
use super::super::EnvironmentShell;

pub(in crate::environment_shell) enum Launch {
    Preset(AgentPreset),
    Agent {
        preset: AgentPreset,
        prompt: String,
        model: Option<ModelId>,
    },
    Resume {
        preset: AgentPreset,
        resume: Option<ResumeId>,
    },
    Command {
        program: String,
        args: Vec<String>,
        vars: Vec<(String, String)>,
    },
    Task(ProjectTask),
}

impl From<RelaunchIntent> for Launch {
    fn from(intent: RelaunchIntent) -> Self {
        LaunchIntent::from(intent).into()
    }
}

impl From<LaunchIntent> for Launch {
    fn from(intent: LaunchIntent) -> Self {
        match intent {
            LaunchIntent::Preset { preset } => Self::Preset(preset),
            LaunchIntent::Agent {
                preset,
                prompt,
                model,
            } => Self::Agent {
                preset,
                prompt,
                model,
            },
            LaunchIntent::Resume { preset, resume } => Self::Resume { preset, resume },
            LaunchIntent::Task { task } => Self::Task(task),
        }
    }
}

impl TryFrom<Launch> for LaunchIntent {
    type Error = Launch;

    fn try_from(launch: Launch) -> Result<Self, Launch> {
        match launch {
            Launch::Preset(preset) => Ok(Self::Preset { preset }),
            Launch::Agent {
                preset,
                prompt,
                model,
            } => Ok(Self::Agent {
                preset,
                prompt,
                model,
            }),
            Launch::Resume { preset, resume } => Ok(Self::Resume { preset, resume }),
            Launch::Task(task) => Ok(Self::Task { task }),
            command @ Launch::Command { .. } => Err(command),
        }
    }
}

impl EnvironmentShell {
    pub(super) fn launch_vars(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        extra: Vec<(String, String)>,
        cx: &App,
    ) -> Vec<(String, String)> {
        let mut vars = self.bus_vars(env, id, cx);
        vars.extend(extra);
        vars
    }

    pub(super) fn launch_site(
        &self,
        env: &EnvironmentName,
        cwd: Option<&Path>,
    ) -> (Option<WslPath>, ShellSite) {
        let host = self.env_host(env);
        let wsl = cwd.filter(|_| host.is_local()).and_then(parse_wsl_path);
        let site = ShellSite::of(&host, wsl.is_some());
        (wsl, site)
    }
}
