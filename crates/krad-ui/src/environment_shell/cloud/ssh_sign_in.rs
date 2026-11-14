use gpui_kit::{Context, Window};
use krad_domain::{CloudRunnerId, EnvironmentName, RunnerState, TerminalId};
use std::collections::HashMap;

use super::super::activity::UtilityTab;
use super::super::live_terminal::{terminal_ref, LiveTerminal};
use super::super::EnvironmentShell;

const SIGN_IN_LABEL: &str = "Sign in";

#[derive(Debug, Default)]
pub(in crate::environment_shell) struct RunnerSignIns(
    HashMap<CloudRunnerId, (EnvironmentName, TerminalId)>,
);

#[derive(Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum SignInMove {
    Keep,
    Drop(EnvironmentName, TerminalId),
    Leave,
}

impl RunnerSignIns {
    pub(in crate::environment_shell) fn opened(
        &mut self,
        runner: CloudRunnerId,
        terminal: (EnvironmentName, TerminalId),
    ) {
        self.0.insert(runner, terminal);
    }

    pub(in crate::environment_shell) fn shown(
        &self,
        runner: CloudRunnerId,
    ) -> Option<&(EnvironmentName, TerminalId)> {
        self.0.get(&runner)
    }

    pub(in crate::environment_shell) fn moved(
        &mut self,
        runner: CloudRunnerId,
        state: &RunnerState,
    ) -> SignInMove {
        if matches!(state, RunnerState::SigningIn | RunnerState::Unknown) {
            return SignInMove::Keep;
        }
        let Some((env, id)) = self.0.remove(&runner) else {
            return SignInMove::Keep;
        };
        if matches!(state, RunnerState::Failed { .. }) {
            return SignInMove::Leave;
        }
        SignInMove::Drop(env, id)
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn runner_sign_in_wanted(
        &mut self,
        runner: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        let Some(destination) = self
            .runtime
            .settings
            .cloud
            .find(runner)
            .map(|found| found.destination.to_string())
        else {
            return;
        };
        let label = format!("{SIGN_IN_LABEL} · {destination}");
        let Some((env, id)) = self.open_helper_terminal(label, cx) else {
            return;
        };
        let view = self
            .runtime
            .handle
            .open_runner_ssh_sign_in(terminal_ref(&env, id), runner);
        self.keep_live_session(&env, id, LiveTerminal::new(view, None), cx);
        self.cloud.sign_ins.opened(runner, (env, id));
        self.show_utilities(UtilityTab::Terminals, cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn runner_sign_in_moved(
        &mut self,
        runner: CloudRunnerId,
        state: &RunnerState,
    ) {
        if let SignInMove::Drop(env, id) = self.cloud.sign_ins.moved(runner, state) {
            self.drop_sign_in_terminal(&env, id);
        }
    }

    pub(in crate::environment_shell) fn runner_sign_in_shown(&self, runner: CloudRunnerId) -> bool {
        self.cloud.sign_ins.shown(runner).is_some()
    }

    pub(in crate::environment_shell) fn show_runner_sign_in(
        &mut self,
        runner: CloudRunnerId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((env, id)) = self.cloud.sign_ins.shown(runner).cloned() else {
            return;
        };
        self.close_settings(window, cx);
        if env == self.active {
            self.show_system_terminal(id, window, cx);
        }
        self.show_utilities(UtilityTab::Terminals, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sign_in_tab_goes_once_the_runner_moves_on_and_stays_to_read_a_failure() {
        let runner = CloudRunnerId(3);
        let env = EnvironmentName::new("main".to_owned());
        let mut sign_ins = RunnerSignIns::default();
        sign_ins.opened(runner, (env.clone(), TerminalId(9)));
        assert_eq!(
            sign_ins.moved(runner, &RunnerState::SigningIn),
            SignInMove::Keep
        );
        assert!(sign_ins.shown(runner).is_some());
        assert_eq!(
            sign_ins.moved(runner, &RunnerState::Connecting),
            SignInMove::Drop(env.clone(), TerminalId(9))
        );
        assert!(sign_ins.shown(runner).is_none());
        sign_ins.opened(runner, (env, TerminalId(10)));
        let failed = RunnerState::Failed {
            reason: "ssh exited with code 255".to_owned(),
        };
        assert_eq!(sign_ins.moved(runner, &failed), SignInMove::Leave);
        assert!(sign_ins.shown(runner).is_none());
        assert_eq!(
            sign_ins.moved(runner, &RunnerState::Connecting),
            SignInMove::Keep
        );
    }
}
