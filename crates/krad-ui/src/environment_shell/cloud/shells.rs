use gpui_kit::Context;
use krad_domain::{EnvironmentName, TerminalId, ToastKind};
use krad_session::PtyOpener;

use super::super::chat::StartError;
use super::super::sessions::Launch;
use super::super::EnvironmentShell;
use super::stops::REMOVED_RUNNER;
use super::terminal::{CloudProgram, CloudTerminal};
use super::NodeSite;

pub(in crate::environment_shell) enum CloudShell {
    Env,
    Waiting,
    Failed(String),
    Ready { opener: PtyOpener, types_line: bool },
}

fn cloud_program(launch: &Launch) -> CloudProgram {
    match launch {
        Launch::Command {
            program,
            args,
            vars,
        } => CloudProgram {
            program: program.clone(),
            args: args.clone(),
            vars: vars.clone(),
        },
        Launch::Preset(_)
        | Launch::Agent { .. }
        | Launch::Resume { .. }
        | Launch::SignIn { .. }
        | Launch::Task(_) => CloudProgram::login_shell(),
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn cloud_shell(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        launch: &Launch,
    ) -> CloudShell {
        if let Some(opener) = self.cloud.openers.remove(&(env.clone(), id)) {
            return CloudShell::Ready {
                opener,
                types_line: false,
            };
        }
        if matches!(launch, Launch::SignIn { .. }) {
            return CloudShell::Env;
        }
        let site = match self.node_site(env, id) {
            NodeSite::Env => return CloudShell::Env,
            NodeSite::Waiting(runner) if self.runner_removed_from_settings(runner) => {
                return CloudShell::Failed(REMOVED_RUNNER.to_owned());
            }
            NodeSite::Waiting(runner) => {
                self.cloud.wanted.insert(runner);
                return CloudShell::Waiting;
            }
            NodeSite::Cloud(site) => site,
        };
        let start = match self.cloud_process_start(env, id, &site) {
            Ok(start) => start,
            Err(StartError::Waiting) => return CloudShell::Waiting,
            Err(error) => return CloudShell::Failed(error.to_string()),
        };
        let mut program = cloud_program(launch);
        program.vars.extend(self.cloud_bus_vars(env, id));
        let terminal = CloudTerminal {
            site,
            process: start.id,
            program,
        };
        CloudShell::Ready {
            opener: terminal.opener(),
            types_line: start.fresh,
        }
    }

    pub(in crate::environment_shell) fn connect_wanted_runners(&mut self, cx: &mut Context<Self>) {
        for runner in std::mem::take(&mut self.cloud.wanted) {
            self.ensure_runner_ready(runner, cx);
        }
        self.check_cloud_processes(cx);
        self.service_runner_sign_ins(cx);
        self.watch_runner_buses(cx);
    }

    pub(in crate::environment_shell) fn report_cloud_shell(&mut self, error: String) {
        self.push_toast(ToastKind::Error, error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_given_command_runs_as_the_cloud_process_and_the_rest_in_a_login_shell() {
        let command = Launch::Command {
            program: "cargo".to_owned(),
            args: vec!["test".to_owned()],
            vars: Vec::new(),
        };
        assert_eq!(cloud_program(&command).program, "cargo");
        let shell = Launch::Preset(krad_domain::AgentPreset::Shell);
        assert_eq!(cloud_program(&shell), CloudProgram::login_shell());
    }
}
