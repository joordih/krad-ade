use krad_chat::{new_opencode_session_id, OpencodeServer};
use krad_domain::{
    parse_wsl_path, resume_line, AgentPreset, EnvironmentName, ModelId, ProjectTask, ResumeId,
    TerminalId, WslPath,
};
use krad_host::{HostError, HostLink};
use krad_session::{default_shell, wsl_shell_in, PtyOpener};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::agent_hooks::AgentRole;
use super::super::host::{remote_terminal, remote_terminal_opener};
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
    SignIn {
        program: String,
        args: Vec<String>,
    },
    Task(ProjectTask),
}

impl Launch {
    pub(super) const fn harness(&self) -> Option<AgentPreset> {
        match self {
            Self::Preset(preset) | Self::Agent { preset, .. } | Self::Resume { preset, .. }
                if preset.is_agent() =>
            {
                Some(*preset)
            }
            Self::Preset(_)
            | Self::Agent { .. }
            | Self::Resume { .. }
            | Self::Command { .. }
            | Self::SignIn { .. }
            | Self::Task(_) => None,
        }
    }

    pub(super) fn model(&self) -> Option<ModelId> {
        match self {
            Self::Agent { model, .. } => model.clone(),
            Self::Preset(_)
            | Self::Resume { .. }
            | Self::Command { .. }
            | Self::SignIn { .. }
            | Self::Task(_) => None,
        }
    }
}

pub(super) struct ResolvedLaunch {
    pub(super) program: String,
    pub(super) args: Vec<String>,
    pub(super) agent: Option<(String, Option<String>)>,
    pub(super) vars: Vec<(String, String)>,
    pub(super) shell: LaunchShell,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LaunchShell {
    Login,
    Given,
    Local,
}

pub(super) struct Placed {
    pub(super) program: String,
    pub(super) args: Vec<String>,
    pub(super) cwd: Option<PathBuf>,
    pub(super) vars: Vec<(String, String)>,
    pub(super) opener: Option<PtyOpener>,
}

impl Placed {
    pub(super) fn on(
        self,
        link: &HostLink,
        shell: LaunchShell,
        in_wsl: bool,
    ) -> Result<Self, HostError> {
        if shell == LaunchShell::Local {
            return Ok(self);
        }
        let given =
            (shell == LaunchShell::Given).then_some((self.program.as_str(), self.args.as_slice()));
        if let Some(opener) = remote_terminal_opener(link, given, self.cwd.as_deref(), &self.vars) {
            return Ok(Self {
                cwd: None,
                vars: Vec::new(),
                opener: Some(opener),
                ..self
            });
        }
        Ok(
            match remote_terminal(link, given, self.cwd.as_deref(), &self.vars)? {
                Some((program, args)) => Self {
                    program,
                    args,
                    cwd: None,
                    vars: Vec::new(),
                    opener: None,
                },
                None if in_wsl => Self { cwd: None, ..self },
                None => self,
            },
        )
    }
}

impl EnvironmentShell {
    pub(super) fn launch_vars(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        extra: Vec<(String, String)>,
        attached: Option<&(Arc<OpencodeServer>, ResumeId)>,
    ) -> Vec<(String, String)> {
        let mut vars = self.bus_vars(env, id);
        vars.extend(extra);
        if let Some((server, _)) = attached {
            vars.extend(server.client_env());
        }
        vars
    }

    pub(super) fn launch_site(
        &self,
        env: &EnvironmentName,
        signs_in: bool,
        cwd: Option<&Path>,
    ) -> (HostLink, Option<WslPath>, ShellSite) {
        let link = if signs_in {
            HostLink::Local
        } else {
            self.env_link(env)
        };
        let wsl = cwd.filter(|_| link.is_local()).and_then(parse_wsl_path);
        let site = ShellSite::of(&link, wsl.is_some());
        (link, wsl, site)
    }

    pub(super) fn opencode_attach(
        &self,
        launch: &Launch,
        harness: Option<AgentPreset>,
        site: ShellSite,
    ) -> Option<(Arc<OpencodeServer>, ResumeId)> {
        if !harness.is_some_and(AgentPreset::attaches_to_server) || site != ShellSite::Native {
            return None;
        }
        let server = self.opencode_server()?.clone();
        let resume = match launch {
            Launch::Resume {
                resume: Some(resume),
                ..
            } => resume.clone(),
            Launch::Preset(_)
            | Launch::Agent { .. }
            | Launch::Resume { .. }
            | Launch::Command { .. }
            | Launch::SignIn { .. }
            | Launch::Task(_) => new_opencode_session_id(),
        };
        Some((server, resume))
    }

    pub(super) fn resolve_launch(
        &self,
        env: &EnvironmentName,
        launch: Launch,
        wsl: Option<&WslPath>,
        site: ShellSite,
        role: AgentRole,
    ) -> ResolvedLaunch {
        let shell = || match wsl {
            Some(folder) => wsl_shell_in(&folder.distro, &folder.linux),
            None => default_shell(),
        };
        match launch {
            Launch::Preset(preset) => {
                let (program, args) = shell();
                ResolvedLaunch {
                    program,
                    args,
                    agent: self.agent_launch_line(env, preset, site, "", None, role),
                    vars: Vec::new(),
                    shell: LaunchShell::Login,
                }
            }
            Launch::Agent {
                preset,
                prompt,
                model,
            } => {
                let (program, args) = shell();
                ResolvedLaunch {
                    program,
                    args,
                    agent: self.agent_launch_line(env, preset, site, &prompt, model.as_ref(), role),
                    vars: Vec::new(),
                    shell: LaunchShell::Login,
                }
            }
            Launch::Resume { preset, resume } => {
                let (program, args) = shell();
                let quoting = site.quoting();
                let agent = self
                    .agent_launch_line(env, preset, site, "", None, role)
                    .map(|(line, after_start)| match &resume {
                        Some(id) => (resume_line(&line, preset, id, quoting), after_start),
                        None => (line, after_start),
                    });
                ResolvedLaunch {
                    program,
                    args,
                    agent,
                    vars: Vec::new(),
                    shell: LaunchShell::Login,
                }
            }
            Launch::Command {
                program,
                args,
                vars,
            } => ResolvedLaunch {
                program,
                args,
                agent: None,
                vars,
                shell: LaunchShell::Given,
            },
            Launch::SignIn { program, args } => ResolvedLaunch {
                program,
                args,
                agent: None,
                vars: Vec::new(),
                shell: LaunchShell::Local,
            },
            Launch::Task(task) => {
                let (program, args) = shell();
                ResolvedLaunch {
                    program,
                    args,
                    agent: Some((task.command_line(site.quoting()), None)),
                    vars: Vec::new(),
                    shell: LaunchShell::Login,
                }
            }
        }
    }
}
