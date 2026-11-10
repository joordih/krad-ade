use gpui_kit::base::TextSelection;
use gpui_kit::{Context, MouseDownEvent, Window};
use krad_chat::Speaker;
use krad_domain::TerminalId;

use super::super::chat::AnswerPart;
use super::super::context_menu::{MenuAction, MenuEntry};
use super::super::EnvironmentShell;
use super::answer::answer_text;
use super::answer_blocks::code_of;
use super::thread::question_text;
use crate::components::IconName;

const COPY_LABEL: &str = "Copy";
const COPY_MESSAGE_LABEL: &str = "Copy message";
const COPY_CODE_LABEL: &str = "Copy code";
const COPY_SELECTION_KEY: &str = "chat-menu-copy-selection";
const COPY_MESSAGE_KEY: &str = "chat-menu-copy-message";
const COPY_CODE_KEY: &str = "chat-menu-copy-code";
const MENU_KEYS: [&str; 3] = [COPY_SELECTION_KEY, COPY_MESSAGE_KEY, COPY_CODE_KEY];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct ChatCopy {
    pub(in crate::environment_shell) key: &'static str,
    pub(in crate::environment_shell) text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CodeSpot {
    pub(super) part: usize,
    pub(super) block: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ChatSpot {
    pub(super) chat: TerminalId,
    pub(super) message: usize,
    pub(super) code: Option<CodeSpot>,
}

fn copy_entry(label: &str, key: &'static str, text: String) -> MenuEntry {
    MenuEntry::item(
        label,
        Some(IconName::Copy),
        MenuAction::CopyChat(ChatCopy { key, text }),
    )
}

fn chat_menu_entries(
    selected: String,
    message: Option<String>,
    code: Option<String>,
) -> Vec<MenuEntry> {
    let nothing_selected = selected.trim().is_empty();
    let mut entries =
        vec![copy_entry(COPY_LABEL, COPY_SELECTION_KEY, selected).disabled_when(nothing_selected)];
    entries.extend(message.map(|text| copy_entry(COPY_MESSAGE_LABEL, COPY_MESSAGE_KEY, text)));
    entries.extend(code.map(|text| copy_entry(COPY_CODE_LABEL, COPY_CODE_KEY, text)));
    entries
}

impl EnvironmentShell {
    pub(super) fn open_chat_menu(
        &mut self,
        spot: ChatSpot,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = TextSelection::selected_text(window, cx);
        let (message, code) = self.chat_copies(spot);
        self.copy_feedback.update(cx, |feedback, _| {
            for key in MENU_KEYS {
                feedback.forget(key);
            }
        });
        self.open_menu_at(event, chat_menu_entries(selected, message, code), cx);
    }

    fn chat_copies(&self, spot: ChatSpot) -> (Option<String>, Option<String>) {
        let Some(message) = self
            .envs
            .get(&self.active)
            .and_then(|live| live.chats.get(&spot.chat))
            .and_then(|chat| chat.messages.get(spot.message))
        else {
            return (None, None);
        };
        let text = match message.speaker {
            Speaker::User => question_text(&message.text).map(str::to_owned),
            Speaker::Assistant => Some(answer_text(message)).filter(|text| !text.is_empty()),
        };
        let code = spot
            .code
            .and_then(|code| match message.parts.get(code.part)? {
                AnswerPart::Text(text) => code_of(text, code.block).map(str::to_owned),
                AnswerPart::Thinking(_) | AnswerPart::Tool(_) | AnswerPart::Ask(_) => None,
            });
        (text, code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(entries: &[MenuEntry]) -> Vec<(&str, bool, ChatCopy)> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Item {
                    label,
                    disabled,
                    action: MenuAction::CopyChat(copy),
                    ..
                } => Some((label.as_str(), *disabled, copy.clone())),
                _ => None,
            })
            .collect()
    }

    fn copy(key: &'static str, text: &str) -> ChatCopy {
        ChatCopy {
            key,
            text: text.to_owned(),
        }
    }

    #[test]
    fn copy_is_disabled_without_a_selection() {
        let entries = chat_menu_entries(" \n".to_owned(), Some("Hi".to_owned()), None);
        assert_eq!(
            summary(&entries),
            vec![
                (COPY_LABEL, true, copy(COPY_SELECTION_KEY, " \n")),
                (COPY_MESSAGE_LABEL, false, copy(COPY_MESSAGE_KEY, "Hi")),
            ]
        );
    }

    #[test]
    fn copy_code_shows_only_over_a_code_block() {
        let entries = chat_menu_entries(
            "ls".to_owned(),
            Some("Run ls".to_owned()),
            Some("ls -la".to_owned()),
        );
        assert_eq!(
            summary(&entries),
            vec![
                (COPY_LABEL, false, copy(COPY_SELECTION_KEY, "ls")),
                (COPY_MESSAGE_LABEL, false, copy(COPY_MESSAGE_KEY, "Run ls")),
                (COPY_CODE_LABEL, false, copy(COPY_CODE_KEY, "ls -la")),
            ]
        );
    }
}
