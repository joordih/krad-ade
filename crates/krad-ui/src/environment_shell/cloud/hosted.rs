use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use krad_domain::{
    CloudRunnerId, Effort, EnvironmentName, GroupId, ModelId, Prompt, RuntimeId, ToastKind,
};
use krad_protocol::{AgentRef, ChatCommand, Command, HostedSummary};
use std::str::FromStr as _;

use super::super::runtime_view::HostedFailure;
use super::super::EnvironmentShell;

mod pane;
mod rows;

const MODEL_COMMAND: &str = "/model";
const EFFORT_COMMAND: &str = "/effort";
const COMPOSER_ROWS: usize = 2;
const COMPOSER_PLACEHOLDER: &str = "Message the cloud session";
const FAILED_PREFIX: &str = "The cloud session could not start on";
const UNREACHED: &str = "The cloud runner is not connected; the message was not sent";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct HostedChatKey {
    pub(in crate::environment_shell) runner: CloudRunnerId,
    pub(in crate::environment_shell) chat: AgentRef,
}

pub(super) struct HostedComposer {
    key: HostedChatKey,
    input: Entity<TextareaState>,
    _events: Subscription,
}

pub(in crate::environment_shell) fn hosted_command(chat: &AgentRef, text: &str) -> ChatCommand {
    let text = text.trim();
    let argument = |command: &str| {
        text.strip_prefix(command)
            .filter(|rest| rest.starts_with(char::is_whitespace))
            .map(str::trim)
    };
    if let Some(model) = argument(MODEL_COMMAND).and_then(ModelId::parse) {
        return ChatCommand::SetModel {
            chat: chat.clone(),
            model,
        };
    }
    if let Some(effort) = argument(EFFORT_COMMAND).and_then(|name| Effort::from_str(name).ok()) {
        return ChatCommand::SetEffort {
            chat: chat.clone(),
            effort,
        };
    }
    ChatCommand::Send {
        chat: chat.clone(),
        prompt: Prompt::from(text),
    }
}

pub(in crate::environment_shell) fn hosted_in<'a>(
    hosted: impl Iterator<Item = (CloudRunnerId, &'a HostedSummary)>,
    environment: &EnvironmentName,
    group: Option<GroupId>,
) -> Vec<(CloudRunnerId, &'a HostedSummary)> {
    hosted
        .filter(|(_, summary)| {
            summary.origin.as_ref().is_some_and(|origin| {
                origin.environment == *environment && Some(origin.group) == group
            })
        })
        .collect()
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn post_hosted(
        &mut self,
        runner: CloudRunnerId,
        command: ChatCommand,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client_for(RuntimeId::Runner(runner), cx) else {
            return self.toast(ToastKind::Warning, UNREACHED, cx);
        };
        client.post(Command::Chat(command));
    }

    pub(in crate::environment_shell) fn hosted_failed(
        &mut self,
        runner: CloudRunnerId,
        failure: &HostedFailure,
        cx: &mut Context<Self>,
    ) {
        let text = format!(
            "{FAILED_PREFIX} {}: {}",
            self.runner_name(runner),
            failure.message
        );
        self.toast(ToastKind::Error, &text, cx);
    }

    pub(in crate::environment_shell) fn leave_hosted_chat(&mut self) {
        self.cloud.shown = None;
        self.cloud.composer = None;
    }

    fn show_hosted_chat(
        &mut self,
        key: HostedChatKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(COMPOSER_ROWS, COMPOSER_ROWS)
                .submit_on_enter(true)
                .placeholder(COMPOSER_PLACEHOLDER)
        });
        let events = cx.subscribe_in(
            &input,
            window,
            move |this: &mut Self, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { shift: false, .. } = event {
                    this.send_hosted(window, cx);
                }
            },
        );
        self.cloud.composer = Some(HostedComposer {
            key: key.clone(),
            input,
            _events: events,
        });
        self.cloud.shown = Some(key);
        cx.notify();
    }

    fn send_hosted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(composer) = &self.cloud.composer else {
            return;
        };
        let text = composer.input.read(cx).value().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let key = composer.key.clone();
        composer
            .input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.post_hosted(key.runner, hosted_command(&key.chat, &text), cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
