use gpui_kit::{AppContext as _, Context, Window};
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    chat_note_markdown, chat_note_stem, exchange_of, fork_plan, EnvironmentView, ForkPlan, Prompt,
    TerminalId, CHAT_NOTES_FOLDER, VAULT_DIR,
};
use krad_repo::add_note;

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::palette::{PALETTE_TOP, PALETTE_W};
use super::super::EnvironmentShell;
use super::GroupChat;
use crate::components::IconName;

const REOPEN_LABEL: &str = "Reopen chat";
const FORK_LABEL: &str = "Fork from this message";
const EXPORT_EXCHANGE_LABEL: &str = "Export this exchange to a note";
const EXPORT_CHAT_LABEL: &str = "Export chat to a note";
const EXPORTED_TEXT: &str = "Exported to";
const NOT_INDEXED_TEXT: &str = "This chat's history is not read yet; try again in a moment";
const NO_GROUP_TEXT: &str = "The chat belongs to no group with a vault for notes";
const EXCERPT_SUFFIX: &str = "excerpt";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum ChatHitAction {
    Reopen(TerminalId),
    Fork(TerminalId, usize),
    ExportExchange(TerminalId, usize),
    ExportChat(TerminalId),
}

#[derive(Clone, Copy)]
enum Exported {
    Exchange(usize),
    Whole,
}

fn hit_entries(id: TerminalId, message: usize) -> Vec<MenuEntry> {
    [
        (REOPEN_LABEL, IconName::Chat, ChatHitAction::Reopen(id)),
        (
            FORK_LABEL,
            IconName::GitBranch,
            ChatHitAction::Fork(id, message),
        ),
        (
            EXPORT_EXCHANGE_LABEL,
            IconName::Note,
            ChatHitAction::ExportExchange(id, message),
        ),
        (
            EXPORT_CHAT_LABEL,
            IconName::Note,
            ChatHitAction::ExportChat(id),
        ),
    ]
    .into_iter()
    .map(|(label, icon, action)| MenuEntry::item(label, Some(icon), MenuAction::ChatHit(action)))
    .collect()
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_chat_hit_menu(
        &mut self,
        id: TerminalId,
        message: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let width = f32::from(window.viewport_size().width);
        let left = ((width - PALETTE_W) / 2.0).max(0.0);
        self.open_menu(
            (left, PALETTE_TOP),
            MenuPlacement::Below,
            hit_entries(id, message),
            cx,
        );
    }

    pub(in crate::environment_shell) fn run_chat_hit_action(
        &mut self,
        action: ChatHitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            ChatHitAction::Reopen(id) => self.reopen_chat(id, window, cx),
            ChatHitAction::Fork(id, message) => self.fork_chat(id, message, cx),
            ChatHitAction::ExportExchange(id, message) => {
                self.export_chat(id, Exported::Exchange(message), cx);
            }
            ChatHitAction::ExportChat(id) => self.export_chat(id, Exported::Whole, cx),
        }
    }

    fn group_chat(&self, id: TerminalId) -> Option<GroupChat> {
        self.group_chats().into_iter().find(|chat| chat.id == id)
    }

    fn reopen_chat(&mut self, id: TerminalId, window: &mut Window, cx: &mut Context<Self>) {
        self.set_view(EnvironmentView::Sessions, cx);
        self.show_terminal_tab(id, window, cx);
    }

    fn fork_chat(&mut self, id: TerminalId, message: usize, cx: &mut Context<Self>) {
        let (Some(chat), Some(document)) = (self.group_chat(id), self.indexed_chat(id)) else {
            self.push_toast(NOT_INDEXED_TEXT.to_owned());
            return;
        };
        let harness_name = self.settings.acp.harness_name(chat.harness);
        let plan = fork_plan(
            &chat.title,
            &harness_name,
            document.messages(),
            message,
            chat.harness.forks(),
        );
        let (session, prompts) = match plan {
            ForkPlan::Whole => (LiveSession::Fork(chat.resume), Vec::new()),
            ForkPlan::Seeded(seed) => (
                LiveSession::Start(new_session_id()),
                vec![Prompt::new(&seed, Vec::new())],
            ),
        };
        let env = self.active.clone();
        let group = self.active_group().map(|group| group.id);
        self.set_view(EnvironmentView::Sessions, cx);
        self.spawn_seeded_chat(&env, group, chat.harness, session, prompts, cx);
    }

    fn export_chat(&mut self, id: TerminalId, exported: Exported, cx: &mut Context<Self>) {
        let (Some(chat), Some(document)) = (self.group_chat(id), self.indexed_chat(id)) else {
            self.push_toast(NOT_INDEXED_TEXT.to_owned());
            return;
        };
        let Some(root) = self.active_group().map(|group| group.root.clone()) else {
            self.push_toast(NO_GROUP_TEXT.to_owned());
            return;
        };
        let messages = document.messages();
        let (title, kept) = match exported {
            Exported::Whole => (chat.title.clone(), messages),
            Exported::Exchange(message) => (
                format!("{} {EXCERPT_SUFFIX}", chat.title),
                messages
                    .get(exchange_of(messages, message))
                    .unwrap_or_default(),
            ),
        };
        let harness_name = self.settings.acp.harness_name(chat.harness);
        let text = chat_note_markdown(&title, &harness_name, kept);
        let stem = chat_note_stem(&title);
        let link = self.active_link();
        let written =
            cx.background_spawn(
                async move { add_note(&link, &root, CHAT_NOTES_FOLDER, &stem, &text) },
            );
        cx.spawn(async move |this, cx| {
            let written = written.await;
            this.update(cx, |shell, cx| {
                match written {
                    Ok(path) => {
                        shell.push_toast(format!("{EXPORTED_TEXT} {VAULT_DIR}/{path}"));
                        shell.refresh_vault(cx);
                    }
                    Err(error) => shell.toast_failure(&error),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hit_offers_reopening_forking_and_both_exports_for_its_message() {
        let actions: Vec<MenuAction> = hit_entries(TerminalId(7), 3)
            .into_iter()
            .filter_map(|entry| match entry {
                MenuEntry::Item { action, .. } => Some(action),
                _ => None,
            })
            .collect();
        assert_eq!(
            actions,
            [
                MenuAction::ChatHit(ChatHitAction::Reopen(TerminalId(7))),
                MenuAction::ChatHit(ChatHitAction::Fork(TerminalId(7), 3)),
                MenuAction::ChatHit(ChatHitAction::ExportExchange(TerminalId(7), 3)),
                MenuAction::ChatHit(ChatHitAction::ExportChat(TerminalId(7))),
            ]
        );
    }
}
