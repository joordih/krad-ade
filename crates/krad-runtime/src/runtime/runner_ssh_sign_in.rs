use krad_domain::{CloudRunnerId, ProcessRun, RunnerState};
use krad_protocol::{AgentRef, EventKind};
use std::sync::Arc;

use super::Runtime;
use crate::hosts::{sign_in_ended, sign_in_launch, Attempt, SIGN_IN_NOT_KEPT};
use crate::terminals::{Shared, Terminals};

const NO_SIGN_IN_WAITING: &str = "No SSH sign-in is waiting for this cloud runner";
const SIGN_IN_CLOSED: &str =
    "The SSH sign-in terminal was closed before ssh signed in. Reconnect to try again.";

impl Runtime {
    pub(super) fn ask_runner_sign_in(&mut self, runner: CloudRunnerId, attempt: Attempt) -> bool {
        if attempt == Attempt::AfterSignIn {
            self.fail_runner(runner, SIGN_IN_NOT_KEPT.to_owned());
            return true;
        }
        let signs_in = self
            .runner_link(runner)
            .as_ref()
            .and_then(sign_in_launch)
            .is_some();
        if !signs_in {
            return false;
        }
        tracing::info!(%runner, "the cloud runner asks to sign in over SSH");
        self.runners.ssh_sign_ins.want(runner);
        let shown = self.runners.set(runner, RunnerState::SigningIn);
        self.show_runner(shown);
        self.outbox.push(EventKind::RunnerSignInWanted { runner });
        true
    }

    #[tracing::instrument(skip_all, fields(%runner))]
    pub(super) fn open_runner_ssh_sign_in(
        &mut self,
        (terminal, shared): (AgentRef, Arc<Shared>),
        runner: CloudRunnerId,
    ) {
        let launch = self.runner_link(runner).as_ref().and_then(sign_in_launch);
        let Some(launch) = launch.filter(|_| self.runners.ssh_sign_ins.open(runner, &terminal))
        else {
            return Terminals::refuse(terminal, &shared, NO_SIGN_IN_WAITING, &mut self.outbox);
        };
        self.terminals
            .open(terminal, launch, shared, &mut self.outbox);
    }

    pub(super) fn follow_runner_sign_ins(&mut self, unread: usize) {
        for (runner, terminal) in self.runners.ssh_sign_ins.opened() {
            let ended = self
                .outbox
                .iter()
                .skip(unread)
                .find_map(|event| match event {
                    EventKind::TerminalEnded(summary) if summary.terminal == terminal => {
                        Some(summary.run.clone())
                    }
                    _ => None,
                });
            let outcome = match ended {
                Some(ProcessRun::Exited { code }) => sign_in_ended(code),
                Some(ProcessRun::Failed { error }) => Err(error),
                Some(ProcessRun::Running) => continue,
                None if self.terminals.runs(&terminal) => continue,
                None => Err(SIGN_IN_CLOSED.to_owned()),
            };
            self.runners.ssh_sign_ins.finish(runner);
            match outcome {
                Ok(()) => {
                    tracing::info!(%runner, "signed in to the cloud runner over SSH");
                    self.terminals.close(&terminal);
                    self.connect_runner(runner, Attempt::AfterSignIn);
                }
                Err(reason) => self.fail_runner(runner, reason),
            }
        }
    }

    pub(super) fn end_runner_sign_in(&mut self, runner: CloudRunnerId) {
        if let Some(terminal) = self.runners.ssh_sign_ins.finish(runner) {
            self.terminals.close(&terminal);
        }
    }

    fn fail_runner(&mut self, runner: CloudRunnerId, reason: String) {
        let shown = self.runners.set(runner, RunnerState::Failed { reason });
        self.show_runner(shown);
    }
}
