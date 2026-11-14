use krad_domain::{CloudRunnerId, PromptAsker, RunnerState};
use krad_host::{HostError, HostLink};

use super::Runtime;
use crate::hosts::{Attempt, SIGN_IN_NOT_KEPT};
use crate::runners::{RunnerDone, SignInRun};

const SIGN_IN_CANCELLED: &str = "Sign-in cancelled";
const SIGN_IN_NOT_STARTED: &str = "Could not start signing in to the cloud runner";

const fn asker(runner: CloudRunnerId) -> PromptAsker {
    PromptAsker::Runner { runner }
}

impl Runtime {
    pub(super) fn ask_runner_sign_in(&mut self, runner: CloudRunnerId, attempt: Attempt) -> bool {
        if attempt == Attempt::AfterSignIn {
            self.fail_runner(runner, SIGN_IN_NOT_KEPT.to_owned());
            return true;
        }
        let Some(HostLink::Remote(ssh)) = self.runner_link(runner) else {
            return false;
        };
        if !ssh.asks_to_sign_in() {
            return false;
        }
        tracing::info!(%runner, "the cloud runner asks to sign in over SSH");
        let run = self.runners.ssh_sign_ins.start(runner);
        self.hosts.admit_prompts(&asker(runner));
        let shown = self.runners.set(runner, RunnerState::SigningIn);
        self.show_runner(shown);
        let started = self.jobs.run(move || {
            let outcome = ssh.sign_in_asking(asker(runner));
            RunnerDone::SignedIn {
                runner,
                run,
                outcome,
            }
            .into()
        });
        if let Err(error) = started {
            tracing::warn!(%error, "a cloud runner sign-in could not start");
            self.runners.ssh_sign_ins.end(runner);
            self.fail_runner(runner, SIGN_IN_NOT_STARTED.to_owned());
        }
        true
    }

    pub(super) fn runner_signed_in(
        &mut self,
        runner: CloudRunnerId,
        run: SignInRun,
        outcome: Result<(), HostError>,
    ) {
        if !self.runners.ssh_sign_ins.finish(runner, run) {
            return;
        }
        self.hosts.decline_prompts(&asker(runner), &mut self.outbox);
        if self.runners.state(runner) != RunnerState::SigningIn {
            return;
        }
        match outcome {
            Ok(()) => {
                tracing::info!(%runner, "signed in to the cloud runner over SSH");
                self.connect_runner(runner, Attempt::AfterSignIn);
            }
            Err(error) => {
                tracing::info!(%runner, %error, "signing in to the cloud runner failed");
                self.fail_runner(runner, error.to_string());
            }
        }
    }

    pub(super) fn runner_prompt_answered(&mut self, asked: Option<&PromptAsker>, given: bool) {
        let Some(&PromptAsker::Runner { runner }) = asked else {
            return;
        };
        if given || !self.end_runner_sign_in(runner) {
            return;
        }
        tracing::info!(%runner, "the cloud runner sign-in was cancelled");
        self.fail_runner(runner, SIGN_IN_CANCELLED.to_owned());
    }

    pub(super) fn end_runner_sign_in(&mut self, runner: CloudRunnerId) -> bool {
        let ended = self.runners.ssh_sign_ins.end(runner);
        if ended {
            self.hosts.decline_prompts(&asker(runner), &mut self.outbox);
        }
        ended
    }

    fn fail_runner(&mut self, runner: CloudRunnerId, reason: String) {
        let shown = self.runners.set(runner, RunnerState::Failed { reason });
        self.show_runner(shown);
    }
}
