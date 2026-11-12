mod listing;
mod reopen;
mod sheet;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use krad_chat::forget_engine_session;
use krad_domain::{
    AgentPreset, Archive, ArchivedChat, ArchivedSessionId, EnvironmentName, GroupId, SessionId,
    TerminalId, ToastKind,
};
use krad_store::{forget_archived_chat_files, gateway_sessions_dir};
use std::time::{Duration, SystemTime};

use super::live_env::live_meta;
use super::EnvironmentShell;

const SWEEP_EVERY: Duration = Duration::from_hours(1);
const SEARCH_PLACEHOLDER: &str = "Search archived sessions and chats…";
const NO_GROUP_TEXT: &str = "Select a group to open its Archive";

pub(super) struct ArchiveSheet {
    group: GroupId,
    search: Entity<InputState>,
    _typing: Subscription,
}

fn forget_files(env: &EnvironmentName, chat: &ArchivedChat) {
    if let Err(error) = forget_archived_chat_files(env, chat.terminal) {
        tracing::warn!(chat = chat.terminal.as_u64(), %error, "archived chat files stayed");
    }
    if chat.harness != AgentPreset::Gateway {
        return;
    }
    let Some(sessions) = gateway_sessions_dir() else {
        return;
    };
    if let Err(error) = forget_engine_session(&sessions, &chat.resume) {
        tracing::warn!(chat = chat.terminal.as_u64(), %error, "archived Gateway session stayed");
    }
}

impl EnvironmentShell {
    pub(super) fn archive_holds(&self, env: &EnvironmentName, terminal: TerminalId) -> bool {
        self.envs
            .get(env)
            .is_some_and(|live| live.data.archive.holds(terminal))
    }

    fn forget_archived(env: &EnvironmentName, chats: Vec<ArchivedChat>, cx: &mut Context<Self>) {
        if chats.is_empty() {
            return;
        }
        let env = env.clone();
        cx.background_spawn(async move {
            for chat in &chats {
                forget_files(&env, chat);
            }
        })
        .detach();
    }

    pub(super) fn archive_session(&mut self, env: &EnvironmentName, session: SessionId) {
        let keeps = self.settings.archive_retention.keeps();
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        if !keeps {
            live.data.archive.leave_session(session);
            return;
        }
        live.data.meta = live_meta(live);
        live.data.archive_session(session, SystemTime::now());
    }

    pub(super) fn archive_chat_tab(&mut self, env: &EnvironmentName, terminal: TerminalId) {
        if !self.settings.archive_retention.keeps() {
            return;
        }
        if let Some(live) = self.envs.get_mut(env) {
            live.data.archive_chat(terminal, SystemTime::now());
        }
    }

    pub(super) fn forget_group_archive(
        &mut self,
        env: &EnvironmentName,
        group: GroupId,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let deleted = live.data.archive.forget_group(group);
        if live
            .archive_sheet
            .as_ref()
            .is_some_and(|sheet| sheet.group == group)
        {
            live.archive_sheet = None;
        }
        Self::forget_archived(env, deleted, cx);
    }

    pub(super) fn forget_environment_archive(
        &mut self,
        env: &EnvironmentName,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let deleted = live.data.archive.forget_all();
        Self::forget_archived(env, deleted, cx);
    }

    pub(super) fn sweep_archive(&mut self, cx: &mut Context<Self>) {
        let retention = self.settings.archive_retention;
        let now = SystemTime::now();
        let expired: Vec<(EnvironmentName, Vec<ArchivedChat>)> = self
            .envs
            .iter_mut()
            .filter_map(|(env, live)| {
                let archive = &mut live.data.archive;
                let before = (archive.sessions.len(), archive.chats.len());
                let deleted = archive.expire(retention, now);
                let after = (archive.sessions.len(), archive.chats.len());
                (after != before).then(|| (env.clone(), deleted))
            })
            .collect();
        if expired.is_empty() {
            return;
        }
        for (env, deleted) in expired {
            Self::forget_archived(&env, deleted, cx);
        }
        cx.notify();
        self.store();
    }

    pub(super) fn start_archive_sweeps(&mut self, cx: &mut Context<Self>) {
        self.sweep_archive(cx);
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(SWEEP_EVERY).await;
            if this.update(cx, Self::sweep_archive).is_err() {
                break;
            }
        })
        .detach();
    }

    pub(super) fn open_archive(
        &mut self,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(SEARCH_PLACEHOLDER));
        let focus = search.clone();
        cx.defer_in(window, move |_, window, cx| {
            focus.update(cx, |field, cx| field.focus(window, cx));
        });
        let typing = cx.subscribe_in(
            &search,
            window,
            |_: &mut Self, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        );
        if let Some(live) = self.envs.get_mut(&self.active) {
            live.archive_sheet = Some(ArchiveSheet {
                group,
                search,
                _typing: typing,
            });
        }
        cx.notify();
    }

    pub(super) fn open_selected_archive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.active_group().map(|group| group.id) {
            Some(group) => self.open_archive(group, window, cx),
            None => self.toast(ToastKind::Warning, NO_GROUP_TEXT, cx),
        }
    }

    fn close_archive(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.envs.get_mut(&self.active) {
            live.archive_sheet = None;
        }
        cx.notify();
    }

    fn delete_archived(
        &mut self,
        forget: impl FnOnce(&mut Archive) -> Vec<ArchivedChat>,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let deleted = forget(&mut live.data.archive);
        Self::forget_archived(&env, deleted, cx);
        cx.notify();
        self.store();
    }

    fn delete_archived_session(&mut self, id: ArchivedSessionId, cx: &mut Context<Self>) {
        self.delete_archived(|archive| archive.delete_session(id), cx);
    }

    fn delete_archived_chat(&mut self, terminal: TerminalId, cx: &mut Context<Self>) {
        self.delete_archived(
            |archive| archive.delete_chat(terminal).into_iter().collect(),
            cx,
        );
    }
}
