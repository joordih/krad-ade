use gpui_kit::{App, Context};
use krad_domain::{
    Edited, EnvironmentChange, EnvironmentData, EnvironmentEdit, EnvironmentName,
    EnvironmentViewPreferences, NodeEdit, NodeField, NodeKind, SessionEdit, TerminalId,
};
use krad_protocol::{Command, EnvironmentCommand};

use super::runtime_view::{EnvironmentCopy, NodesMoved};
use super::{EnvironmentShell, LiveEnv};

impl EnvironmentShell {
    pub(super) fn edit_environment(
        &mut self,
        env: &EnvironmentName,
        edit: impl Into<EnvironmentEdit>,
        cx: &mut App,
    ) -> Edited {
        let edit = edit.into();
        let request = self.runtime.client.next_request();
        let Some(edited) = self.edit_shown(env, cx, |copy| copy.edit(&edit, request)) else {
            return Edited::Ignored;
        };
        drop(
            self.runtime
                .client
                .send_as(request, Command::from((env.clone(), edit))),
        );
        edited
    }

    pub(super) fn edit_shown<R>(
        &mut self,
        env: &EnvironmentName,
        cx: &mut App,
        edit: impl FnOnce(&mut EnvironmentCopy) -> R,
    ) -> Option<R> {
        let live = self.envs.get_mut(env)?;
        self.runtime.edit_shown(env, &mut live.data, cx, edit)
    }

    pub(super) fn set_node(
        &mut self,
        env: &EnvironmentName,
        terminal: TerminalId,
        field: NodeField,
        cx: &mut App,
    ) {
        self.edit_environment(env, NodeEdit::Set { terminal, field }, cx);
    }

    pub(super) fn retitle_environment(
        &mut self,
        env: &EnvironmentName,
        title: Option<String>,
        cx: &mut App,
    ) {
        let shown = title.clone();
        if self
            .edit_shown(env, cx, |copy| copy.set_title(shown))
            .is_none()
        {
            return;
        }
        self.runtime
            .client
            .post(Command::Environments(EnvironmentCommand::Change {
                environment: env.clone(),
                changes: vec![EnvironmentChange::Title { title }],
            }));
    }

    pub(super) fn open_environment(&mut self, mut data: EnvironmentData, cx: &mut App) {
        let name = data.name.clone();
        let preferences = data.take_view();
        self.runtime
            .client
            .post(Command::Environments(EnvironmentCommand::Open {
                environment: Box::new(data.clone()),
            }));
        self.runtime.open_shown(data, cx);
        self.attach_live_env(&name, preferences, cx);
    }

    pub(super) fn attach_live_env(
        &mut self,
        env: &EnvironmentName,
        preferences: EnvironmentViewPreferences,
        cx: &mut App,
    ) {
        let Some(data) = self.runtime.shown(env, cx) else {
            return;
        };
        let focus = preferences
            .focus
            .filter(|id| data.nodes.iter().any(|node| node.id == *id));
        let chats: Vec<TerminalId> = data
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Chat)
            .map(|node| node.id)
            .collect();
        self.envs
            .insert(env.clone(), LiveEnv::new(data, preferences, focus));
        self.open_restored_chats(env, chats, cx);
        if let Some(terminal) = focus {
            self.edit_environment(env, SessionEdit::Reveal { terminal }, cx);
        }
    }

    pub(super) fn environments_reread(&mut self, cx: &mut App) {
        for (name, live) in &mut self.envs {
            if let Some(shown) = self.runtime.view.read(cx).shown.get(name) {
                live.data = std::rc::Rc::clone(shown);
            }
        }
        let missing: Vec<EnvironmentName> = self
            .runtime
            .order
            .iter()
            .filter(|name| !self.envs.contains_key(*name))
            .cloned()
            .collect();
        for name in missing {
            let preferences = EnvironmentViewPreferences::new(name.clone());
            self.attach_live_env(&name, preferences, cx);
        }
        let order = &self.runtime.order;
        self.envs.retain(|name, _| order.contains(name));
        if !self.envs.contains_key(&self.active) {
            if let Some(first) = self.runtime.order.first() {
                self.active = first.clone();
            }
        }
    }

    pub(super) fn environment_heard(&mut self, env: &EnvironmentName) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let gone = |group| live.data.group(group).is_none();
        if live.selected_group.is_some_and(gone) {
            live.selected_group = live.data.groups.first().map(|group| group.id);
        }
        if live
            .archive_sheet
            .as_ref()
            .is_some_and(|sheet| live.data.group(sheet.group).is_none())
        {
            live.archive_sheet = None;
        }
    }

    pub(super) fn nodes_moved(
        &mut self,
        env: &EnvironmentName,
        moved: NodesMoved,
        cx: &mut Context<Self>,
    ) {
        for gone in moved.gone {
            self.forget_live_node(env, gone, cx);
        }
        let Some(live) = self.envs.get(env) else {
            return;
        };
        let came: Vec<TerminalId> = live
            .data
            .nodes
            .iter()
            .filter(|node| moved.came.contains(&node.id) && node.kind == NodeKind::Chat)
            .map(|node| node.id)
            .collect();
        self.open_restored_chats(env, came, cx);
        cx.notify();
    }
}
