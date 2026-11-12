use gpui_kit::Context;
use krad_domain::{AgentPreset, CloudRunnerId, EnvironmentName, TerminalId};

use super::super::EnvironmentShell;
use super::stops::REMOVED_RUNNER;

const CONNECTING_PREFIX: &str = "Connecting to cloud runner";
const RUNNER_PREFIX: &str = "Cloud runner";

fn waits_for_runner(problem: Option<&str>) -> bool {
    problem.is_some_and(|problem| problem.starts_with(CONNECTING_PREFIX))
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn note_cloud_wait(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        runner: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        let text = if self.runner_removed_from_settings(runner) {
            REMOVED_RUNNER.to_owned()
        } else {
            self.ensure_runner_ready(runner, cx);
            format!("{CONNECTING_PREFIX} {}", self.runner_name(runner))
        };
        if let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.problem = Some(text);
        }
    }

    pub(super) fn cloud_chats_on(
        &self,
        runner: CloudRunnerId,
    ) -> Vec<(EnvironmentName, TerminalId)> {
        self.envs
            .iter()
            .flat_map(|(env, live)| {
                live.chats
                    .keys()
                    .filter(move |id| {
                        live.data
                            .session_of(**id)
                            .and_then(|session| live.data.session(session))
                            .and_then(|session| session.cloud.as_ref())
                            .is_some_and(|placement| placement.runner == runner)
                    })
                    .map(move |id| (env.clone(), *id))
            })
            .collect()
    }

    pub(in crate::environment_shell) fn cloud_runner_settled(
        &mut self,
        runner: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        let chats = self.cloud_chats_on(runner);
        if self.runner_link(runner).is_none() {
            self.drop_runner_bus(runner);
            self.fail_pending_cloud_sessions(runner, cx);
            let text = format!(
                "{RUNNER_PREFIX} {}: {}",
                self.runner_name(runner),
                self.runner_state(runner).label()
            );
            for (env, id) in chats {
                if let Some(chat) = self
                    .envs
                    .get_mut(&env)
                    .and_then(|live| live.chats.get_mut(&id))
                {
                    chat.problem = Some(text.clone());
                }
            }
            return;
        }
        self.start_runner_return(runner, cx);
        self.start_pending_cloud_sessions(runner, cx);
        self.run_queued_stops(runner, cx);
        self.check_runner_logins(runner, cx);
        for (env, id) in chats {
            if let Some(chat) = self
                .envs
                .get_mut(&env)
                .and_then(|live| live.chats.get_mut(&id))
                .filter(|chat| waits_for_runner(chat.problem.as_deref()))
            {
                chat.problem = None;
            }
            self.load_history_of(&env, id, cx);
        }
        self.resume_cloud_work(cx);
    }

    pub(super) fn resume_cloud_work(&mut self, cx: &mut Context<Self>) {
        let harnesses: Vec<AgentPreset> =
            AgentPreset::listed(&self.settings.acp.all_agents()).collect();
        for harness in harnesses {
            self.flush_chats_of(harness, cx);
        }
        let envs: Vec<EnvironmentName> = self.envs.keys().cloned().collect();
        for env in envs {
            self.start_deferred_launches(&env);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_connecting_note_clears_once_the_runner_is_ready() {
        assert!(waits_for_runner(Some("Connecting to cloud runner gpu")));
        assert!(!waits_for_runner(Some("claude is not on PATH")));
        assert!(!waits_for_runner(None));
    }
}
