use gpui_kit::{Context, Window};
use krad_domain::{CloudRunnerId, EnvironmentName, GroupId, ToastKind};
use krad_protocol::{AgentRef, HostedSummary};

use super::super::runtime_view::HostedFailure;
use super::super::surface::ChatPlace;
use super::super::EnvironmentShell;

mod frame;
mod lives;
mod rows;
mod terminal;
mod terminal_view;

pub(in crate::environment_shell) use lives::HostedLives;
pub(in crate::environment_shell) use terminal_view::HostedTerminalView;

const FAILED_PREFIX: &str = "The cloud session could not start on";
const UNREACHED: &str = "The cloud runner is not connected";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct HostedChatKey {
    pub(in crate::environment_shell) runner: CloudRunnerId,
    pub(in crate::environment_shell) chat: AgentRef,
}

impl HostedChatKey {
    pub(in crate::environment_shell) fn place(&self) -> ChatPlace {
        ChatPlace::hosted(self.runner, self.chat.environment.clone())
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

    pub(in crate::environment_shell) fn shown_hosted_place(&self) -> Option<ChatPlace> {
        self.cloud.shown.as_ref().map(HostedChatKey::place)
    }

    pub(in crate::environment_shell) fn leave_hosted_chat(&mut self) {
        self.cloud.shown = None;
        self.forget_unshown_hosted_places();
    }

    fn show_hosted_chat(
        &mut self,
        key: HostedChatKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let place = key.place();
        self.follow_hosted_place(&place, cx);
        if self.live(&place).is_none() {
            return self.toast(ToastKind::Warning, UNREACHED, cx);
        }
        self.open_hosted_chat(&place, key.chat.terminal, window, cx);
        self.cloud.terminal = None;
        self.cloud.shown = Some(key);
        self.forget_unshown_hosted_places();
        cx.notify();
    }

    fn forget_unshown_hosted_places(&mut self) {
        let shown = self.shown_hosted_place();
        self.cloud
            .lives
            .retain(|place| shown.as_ref() == Some(place));
    }
}

#[cfg(test)]
mod tests;
