use gpui_kit::{App, Context};
use krad_client::RuntimeClient;
use krad_domain::{
    CloudRunner, CloudRunnerId, EnvironmentName, GroupId, Prompt, RunnerState, SessionId,
    TerminalId,
};
use krad_protocol::{CloudCommand, Command, SentPrompt};

use super::super::runtime_view::RunnerReach;
use super::super::surface::Backend;
use super::super::EnvironmentShell;

const READY: &str = "Ready";
const CONNECTING: &str = "Connecting";
const SIGNING_IN: &str = "Signing in";
const INSTALLING: &str = "Installing";
const NOT_CONNECTED: &str = "Not connected";
const FAILED: &str = "Failed";
const UPDATE_PENDING: &str = "Update pending";
const UNKNOWN: &str = "Unknown";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) enum RunOn {
    #[default]
    Here,
    Runner(CloudRunnerId),
}

impl From<RunOn> for Option<CloudRunnerId> {
    fn from(run_on: RunOn) -> Self {
        match run_on {
            RunOn::Here => None,
            RunOn::Runner(runner) => Some(runner),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct RunOnChoice {
    pub(in crate::environment_shell) run_on: RunOn,
    pub(in crate::environment_shell) label: String,
    pub(in crate::environment_shell) state: Option<&'static str>,
    pub(in crate::environment_shell) open: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct FirstMessage {
    pub(in crate::environment_shell) environment: EnvironmentName,
    pub(in crate::environment_shell) session: SessionId,
    pub(in crate::environment_shell) chat: TerminalId,
    pub(in crate::environment_shell) prompt: Prompt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct RunOnOffer {
    pub(in crate::environment_shell) session: SessionId,
    pub(in crate::environment_shell) group: GroupId,
    pub(in crate::environment_shell) run_on: RunOn,
}

const fn state_label(state: &RunnerState, reach: Option<&RunnerReach>) -> &'static str {
    match (state, reach) {
        (
            RunnerState::Ready { .. } | RunnerState::UpdatePending { .. },
            Some(RunnerReach::ProcessWire),
        ) => UPDATE_PENDING,
        (RunnerState::Ready { .. } | RunnerState::UpdatePending { .. }, _) => READY,
        (RunnerState::Connecting, _) => CONNECTING,
        (RunnerState::SigningIn, _) => SIGNING_IN,
        (RunnerState::Installing, _) => INSTALLING,
        (RunnerState::NotConnected, _) => NOT_CONNECTED,
        (RunnerState::Failed { .. }, _) => FAILED,
        (RunnerState::Unknown, _) => UNKNOWN,
    }
}

const fn takes_sessions(state: &RunnerState) -> bool {
    !matches!(state, RunnerState::Failed { .. } | RunnerState::Unknown)
}

pub(in crate::environment_shell) fn run_on_choices(
    here: String,
    runners: &[CloudRunner],
    reached: impl Fn(CloudRunnerId) -> (RunnerState, Option<RunnerReach>),
) -> Vec<RunOnChoice> {
    let local = RunOnChoice {
        run_on: RunOn::Here,
        label: here,
        state: None,
        open: true,
    };
    let remote = runners.iter().map(|runner| {
        let (state, reach) = reached(runner.id);
        RunOnChoice {
            run_on: RunOn::Runner(runner.id),
            label: runner.name.as_str().to_owned(),
            state: Some(state_label(&state, reach.as_ref())),
            open: takes_sessions(&state),
        }
    });
    std::iter::once(local).chain(remote).collect()
}

pub(in crate::environment_shell) fn remembered_run_on(
    remembered: Option<CloudRunnerId>,
    runners: &[CloudRunner],
) -> RunOn {
    remembered
        .filter(|runner| runners.iter().any(|known| known.id == *runner))
        .map_or(RunOn::Here, RunOn::Runner)
}

pub(in crate::environment_shell) fn post_first_message(
    client: &dyn RuntimeClient,
    run_on: RunOn,
    first: &FirstMessage,
) -> bool {
    let RunOn::Runner(runner) = run_on else {
        return false;
    };
    client.post(Command::Cloud(CloudCommand::Send {
        environment: first.environment.clone(),
        session: first.session,
        runner,
        prompt: Some(SentPrompt {
            chat: first.chat,
            prompt: first.prompt.clone(),
        }),
    }));
    true
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn run_on_here_label(&self) -> String {
        self.envs
            .get(&self.active)
            .map_or_else(String::new, |live| live.data.host.to_string())
    }

    pub(in crate::environment_shell) fn run_on_choices(&self, cx: &App) -> Vec<RunOnChoice> {
        let remotes = &self.runtime.view.read(cx).remotes;
        run_on_choices(
            self.run_on_here_label(),
            &self.runtime.settings.cloud.runners,
            |runner| {
                let reach = remotes.get(runner).map(|remote| remote.reach.clone());
                (self.runner_state(runner, cx), reach)
            },
        )
    }

    pub(in crate::environment_shell) fn run_on_offer(&self, id: TerminalId) -> Option<RunOnOffer> {
        let runners = &self.runtime.settings.cloud.runners;
        if runners.is_empty() || self.shown_here().is_none() {
            return None;
        }
        let live = self.envs.get(&self.active)?;
        let chat = live.chats.get(&id)?;
        let Backend::Structured(structured) = &chat.chat.backend else {
            return None;
        };
        if !chat.chat.messages.is_empty() || !structured.is_fresh() {
            return None;
        }
        let session = live.data.session(live.data.session_of(id)?)?;
        let group = live.data.group(session.group?)?;
        let has_remote = self
            .active_git(&group.root)
            .is_some_and(|git| git.refs.remote().next().is_some());
        if session.cloud.is_some() || !has_remote {
            return None;
        }
        Some(RunOnOffer {
            session: session.id,
            group: group.id,
            run_on: remembered_run_on(live.view.run_on, runners),
        })
    }

    pub(in crate::environment_shell) fn choose_run_on(
        &mut self,
        run_on: RunOn,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.envs.get_mut(&self.active) else {
            return;
        };
        live.view.run_on = run_on.into();
        self.save_view();
        cx.notify();
    }

    pub(in crate::environment_shell) fn first_message_to_cloud(
        &mut self,
        id: TerminalId,
        text: &str,
    ) -> Option<Prompt> {
        let offer = self.run_on_offer(id)?;
        let first = FirstMessage {
            environment: self.active.clone(),
            session: offer.session,
            chat: id,
            prompt: Prompt::new(text, Vec::new()),
        };
        if !post_first_message(self.runtime.client.as_ref(), offer.run_on, &first) {
            return None;
        }
        if let Some(chat) = self
            .envs
            .get_mut(&first.environment)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.mentions.clear();
            chat.attached.clear();
        }
        Some(first.prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_client::RecordedClient;
    use krad_domain::{CloudRunnerName, Linger, SshDestination};

    fn runner(id: u64, name: &str) -> CloudRunner {
        CloudRunner {
            id: CloudRunnerId(id),
            name: CloudRunnerName::try_from(name.to_owned())
                .unwrap_or_else(|_| unreachable!("a runner name")),
            destination: SshDestination::try_from(format!("me@{name}"))
                .unwrap_or_else(|_| unreachable!("an ssh destination")),
            offline_webhooks: krad_domain::OfflineWebhooks::default(),
        }
    }

    fn first() -> FirstMessage {
        FirstMessage {
            environment: EnvironmentName::new("api".to_owned()),
            session: SessionId(3),
            chat: TerminalId(7),
            prompt: Prompt::new("Fix the flaky login test", Vec::new()),
        }
    }

    #[test]
    fn the_selector_lists_this_computer_first_and_every_runner_with_its_state() {
        let runners = [
            runner(1, "gpu"),
            runner(2, "lab"),
            runner(3, "old"),
            runner(4, "vm"),
            runner(5, "far"),
        ];
        let ready = RunnerState::Ready {
            linger: Linger::Enabled,
        };
        let choices = run_on_choices("This computer".to_owned(), &runners, |id| match id.0 {
            1 => (
                ready.clone(),
                Some(RunnerReach::Connected {
                    build: "0.4.2".to_owned(),
                }),
            ),
            2 => (RunnerState::Connecting, None),
            3 => (ready.clone(), Some(RunnerReach::ProcessWire)),
            4 => (RunnerState::SigningIn, None),
            _ => (
                RunnerState::Failed {
                    reason: "no route".to_owned(),
                },
                None,
            ),
        });
        let read: Vec<(RunOn, &str, Option<&str>, bool)> = choices
            .iter()
            .map(|choice| {
                (
                    choice.run_on,
                    choice.label.as_str(),
                    choice.state,
                    choice.open,
                )
            })
            .collect();
        assert_eq!(
            read,
            vec![
                (RunOn::Here, "This computer", None, true),
                (RunOn::Runner(CloudRunnerId(1)), "gpu", Some(READY), true),
                (
                    RunOn::Runner(CloudRunnerId(2)),
                    "lab",
                    Some(CONNECTING),
                    true
                ),
                (
                    RunOn::Runner(CloudRunnerId(3)),
                    "old",
                    Some(UPDATE_PENDING),
                    true
                ),
                (
                    RunOn::Runner(CloudRunnerId(4)),
                    "vm",
                    Some(SIGNING_IN),
                    true
                ),
                (RunOn::Runner(CloudRunnerId(5)), "far", Some(FAILED), false),
            ]
        );
    }

    #[test]
    fn new_chats_run_here_unless_a_known_runner_was_chosen() {
        let runners = [runner(1, "gpu")];
        assert_eq!(RunOn::default(), RunOn::Here);
        assert_eq!(remembered_run_on(None, &runners), RunOn::Here);
        assert_eq!(
            remembered_run_on(Some(CloudRunnerId(1)), &runners),
            RunOn::Runner(CloudRunnerId(1))
        );
        assert_eq!(
            remembered_run_on(Some(CloudRunnerId(9)), &runners),
            RunOn::Here
        );
        assert_eq!(Option::<CloudRunnerId>::from(RunOn::Here), None);
    }

    #[test]
    fn a_first_message_on_a_runner_moves_its_own_session_there_and_here_posts_nothing() {
        let client = RecordedClient::new(Vec::new());
        assert!(!post_first_message(client.as_ref(), RunOn::Here, &first()));
        assert_eq!(client.sent(), Vec::new());
        assert!(post_first_message(
            client.as_ref(),
            RunOn::Runner(CloudRunnerId(2)),
            &first()
        ));
        assert_eq!(
            client.sent(),
            vec![Command::Cloud(CloudCommand::Send {
                environment: EnvironmentName::new("api".to_owned()),
                session: SessionId(3),
                runner: CloudRunnerId(2),
                prompt: Some(SentPrompt {
                    chat: TerminalId(7),
                    prompt: Prompt::new("Fix the flaky login test", Vec::new()),
                }),
            })]
        );
    }
}
