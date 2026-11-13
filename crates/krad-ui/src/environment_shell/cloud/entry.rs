use gpui_kit::{App, Context};
use krad_domain::{CloudRunnerId, GroupId, SessionId, TerminalGroup, ToastKind};

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::EnvironmentShell;
use super::state::runner_label;
use crate::components::IconName;

const NEW_LABEL: &str = "New cloud session…";
const SEND_LABEL: &str = "Send to cloud…";
const NO_RUNNER_REASON: &str = "Add a cloud runner in Settings";
const NO_REMOTE_REASON: &str = "The group has no git remote";
const NO_SESSION_TOAST: &str = "Open a session first";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum CloudMenu {
    New(GroupId, CloudRunnerId),
    Send(SessionId, CloudRunnerId),
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn run_cloud_menu(&self, menu: CloudMenu) {
        match menu {
            CloudMenu::New(group, runner) => self.new_cloud_session(group, runner),
            CloudMenu::Send(session, runner) => self.send_to_cloud(session, runner),
        }
    }

    pub(in crate::environment_shell) fn cloud_session_entry(
        &self,
        group: &TerminalGroup,
        cx: &App,
    ) -> MenuEntry {
        let runners = &self.settings.cloud.runners;
        let Some(first) = runners.first() else {
            return MenuEntry::item(NEW_LABEL, Some(IconName::Cloud), MenuAction::OpenSettings)
                .disabled_when(true)
                .with_detail(NO_RUNNER_REASON);
        };
        let new_in = |runner: CloudRunnerId| MenuAction::Cloud(CloudMenu::New(group.id, runner));
        let has_remote = self
            .active_git(&group.root)
            .is_some_and(|git| git.refs.remote().next().is_some());
        if !has_remote {
            return MenuEntry::item(NEW_LABEL, Some(IconName::Cloud), new_in(first.id))
                .disabled_when(true)
                .with_detail(NO_REMOTE_REASON);
        }
        if let [runner] = runners.as_slice() {
            return MenuEntry::item(NEW_LABEL, Some(IconName::Cloud), new_in(runner.id));
        }
        MenuEntry::submenu(NEW_LABEL, IconName::Cloud, self.runner_entries(new_in, cx))
    }

    fn runner_entries(
        &self,
        action: impl Fn(CloudRunnerId) -> MenuAction,
        cx: &App,
    ) -> Vec<MenuEntry> {
        self.settings
            .cloud
            .runners
            .iter()
            .map(|runner| {
                MenuEntry::item(runner.name.as_str(), None, action(runner.id))
                    .with_detail(runner_label(&self.runner_state(runner.id, cx)))
            })
            .collect()
    }

    pub(in crate::environment_shell) fn new_cloud_session_in_selected(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let group = self.active_group().map(|group| group.id);
        let runner = self.settings.cloud.runners.first().map(|runner| runner.id);
        match (group, runner) {
            (Some(group), Some(runner)) => self.new_cloud_session(group, runner),
            (_, None) => self.toast(ToastKind::Warning, NO_RUNNER_REASON, cx),
            (None, Some(_)) => self.toast(ToastKind::Warning, NO_REMOTE_REASON, cx),
        }
    }

    pub(in crate::environment_shell) fn can_send_to_cloud(&self, session: SessionId) -> bool {
        !self.settings.cloud.runners.is_empty()
            && self
                .envs
                .get(&self.active)
                .and_then(|live| live.data.session(session))
                .is_some_and(|found| found.cloud.is_none() && found.group.is_some())
    }

    pub(in crate::environment_shell) fn offer_send_to_cloud(
        &mut self,
        session: SessionId,
        origin: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        match self.settings.cloud.runners.as_slice() {
            [] => self.toast(ToastKind::Warning, NO_RUNNER_REASON, cx),
            [runner] => {
                let runner = runner.id;
                self.send_to_cloud(session, runner);
            }
            _ => {
                let entries = vec![MenuEntry::submenu(
                    SEND_LABEL,
                    IconName::Cloud,
                    self.runner_entries(
                        |runner| MenuAction::Cloud(CloudMenu::Send(session, runner)),
                        cx,
                    ),
                )];
                self.open_menu(origin, MenuPlacement::Below, entries, cx);
            }
        }
    }

    pub(in crate::environment_shell) fn send_current_to_cloud(&mut self, cx: &mut Context<Self>) {
        let current = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.current_session())
            .map(|session| session.id);
        let runner = self.settings.cloud.runners.first().map(|runner| runner.id);
        match (current, runner) {
            (Some(session), Some(runner)) => self.send_to_cloud(session, runner),
            (None, _) => self.toast(ToastKind::Warning, NO_SESSION_TOAST, cx),
            (Some(_), None) => self.toast(ToastKind::Warning, NO_RUNNER_REASON, cx),
        }
    }

    fn send_to_cloud(&self, session: SessionId, runner: CloudRunnerId) {
        self.send_session_to_cloud(session, runner);
    }
}
