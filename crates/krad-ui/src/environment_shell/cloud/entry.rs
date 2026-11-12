use gpui_kit::{Context, Window};
use krad_domain::{CloudRunnerId, GroupId, SessionId, TerminalGroup, ToastKind};

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::EnvironmentShell;
use super::naming::sent_session_name;
use super::sessions::CloudOrder;
use crate::components::IconName;

const NEW_LABEL: &str = "New cloud session…";
const SEND_LABEL: &str = "Send to cloud…";
const NO_RUNNER_REASON: &str = "Add a cloud runner in Settings";
const NO_REMOTE_REASON: &str = "The group has no git remote";
const NO_SESSION_TOAST: &str = "Open a session first";
const ALREADY_CLOUD_TOAST: &str = "This session already runs in the cloud";
const NO_GROUP_TOAST: &str = "Only a session in a group can go to the cloud";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum CloudMenu {
    New(GroupId, CloudRunnerId),
    Send(SessionId, CloudRunnerId),
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn run_cloud_menu(
        &mut self,
        menu: CloudMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match menu {
            CloudMenu::New(group, runner) => self.new_cloud_session(group, runner, window, cx),
            CloudMenu::Send(session, runner) => self.send_to_cloud(session, runner, window, cx),
        }
    }

    pub(in crate::environment_shell) fn cloud_session_entry(
        &self,
        group: &TerminalGroup,
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
        MenuEntry::submenu(NEW_LABEL, IconName::Cloud, self.runner_entries(new_in))
    }

    fn runner_entries(&self, action: impl Fn(CloudRunnerId) -> MenuAction) -> Vec<MenuEntry> {
        self.settings
            .cloud
            .runners
            .iter()
            .map(|runner| {
                MenuEntry::item(runner.name.as_str(), None, action(runner.id))
                    .with_detail(self.runner_state(runner.id).label())
            })
            .collect()
    }

    pub(in crate::environment_shell) fn new_cloud_session_in_selected(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let group = self.active_group().map(|group| group.id);
        let runner = self.settings.cloud.runners.first().map(|runner| runner.id);
        match (group, runner) {
            (Some(group), Some(runner)) => self.new_cloud_session(group, runner, window, cx),
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.settings.cloud.runners.as_slice() {
            [] => self.toast(ToastKind::Warning, NO_RUNNER_REASON, cx),
            [runner] => {
                let runner = runner.id;
                self.send_to_cloud(session, runner, window, cx);
            }
            _ => {
                let entries = vec![MenuEntry::submenu(
                    SEND_LABEL,
                    IconName::Cloud,
                    self.runner_entries(|runner| {
                        MenuAction::Cloud(CloudMenu::Send(session, runner))
                    }),
                )];
                self.open_menu(origin, MenuPlacement::Below, entries, cx);
            }
        }
    }

    pub(in crate::environment_shell) fn send_current_to_cloud(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.current_session())
            .map(|session| session.id);
        let runner = self.settings.cloud.runners.first().map(|runner| runner.id);
        match (current, runner) {
            (Some(session), Some(runner)) => self.send_to_cloud(session, runner, window, cx),
            (None, _) => self.toast(ToastKind::Warning, NO_SESSION_TOAST, cx),
            (Some(_), None) => self.toast(ToastKind::Warning, NO_RUNNER_REASON, cx),
        }
    }

    fn send_to_cloud(
        &mut self,
        session: SessionId,
        runner: CloudRunnerId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(found) = self
            .envs
            .get(&env)
            .and_then(|live| live.data.session(session))
        else {
            return self.toast(ToastKind::Warning, NO_SESSION_TOAST, cx);
        };
        if found.cloud.is_some() {
            return self.toast(ToastKind::Warning, ALREADY_CLOUD_TOAST, cx);
        }
        let Some(group) = found.group else {
            return self.toast(ToastKind::Warning, NO_GROUP_TOAST, cx);
        };
        let order = CloudOrder {
            name: Some(sent_session_name(&found.name)),
            carried: self.carried_chats(&env, session),
        };
        tracing::info!(%runner, chats = order.carried.len(), "sending a session to the cloud");
        self.create_cloud_session(group, runner, order, window, cx);
    }
}
