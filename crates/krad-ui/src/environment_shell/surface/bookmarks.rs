use gpui_kit::{Context, ElementId, IntoElement, StatefulInteractiveElement};
use krad_chat::Speaker;
use krad_domain::{Bookmark, ChatBookmarks, TerminalId, TurnPrint, TurnSide};

use super::super::chat::ChatMessage;
use super::super::EnvironmentShell;
use super::message_actions::{tinted_action_button, BookmarkAction};
use crate::components::IconName;
use crate::theme::Palette;

const BOOKMARK_PREFIX: &str = "surface-bookmark";
const ADD_TOOLTIP: &str = "Bookmark message";
const REMOVE_TOOLTIP: &str = "Remove bookmark";

pub(super) fn bookmark_of(messages: &[ChatMessage], index: usize) -> Option<Bookmark> {
    let side = match messages.get(index)?.speaker {
        Speaker::User => TurnSide::Question,
        Speaker::Assistant => TurnSide::Answer,
    };
    let question = messages
        .get(..=index)?
        .iter()
        .rev()
        .find(|message| message.speaker == Speaker::User)?;
    let asked = question.text.trim();
    if asked.is_empty() {
        return None;
    }
    Some(Bookmark {
        turn: TurnPrint::from(asked),
        side,
    })
}

impl EnvironmentShell {
    pub(super) fn chat_bookmarks(&self, id: TerminalId) -> Option<&ChatBookmarks> {
        self.envs
            .get(&self.active)?
            .data
            .nodes
            .iter()
            .find(|node| node.id == id)
            .map(|node| &node.bookmarks)
    }

    pub(super) fn bookmark_action(
        &self,
        id: TerminalId,
        index: usize,
        messages: &[ChatMessage],
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Option<BookmarkAction> {
        let bookmark = bookmark_of(messages, index)?;
        let marked = self
            .chat_bookmarks(id)
            .is_some_and(|bookmarks| bookmarks.contains(&bookmark));
        let (tooltip, color) = if marked {
            (REMOVE_TOOLTIP, pal.attention)
        } else {
            (ADD_TOOLTIP, pal.text_muted)
        };
        let key = format!("{BOOKMARK_PREFIX}-{}-{index}", id.as_u64());
        let button = tinted_action_button(
            ElementId::Name(key.into()),
            IconName::Bookmark,
            tooltip,
            color,
            pal,
        )
        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
            this.toggle_bookmark(id, bookmark);
            cx.notify();
        }))
        .into_any_element();
        Some(BookmarkAction { button, marked })
    }

    fn toggle_bookmark(&mut self, id: TerminalId, bookmark: Bookmark) {
        let Some(node) = self
            .envs
            .get_mut(&self.active)
            .and_then(|live| live.data.nodes.iter_mut().find(|node| node.id == id))
        else {
            return;
        };
        node.bookmarks.toggle(bookmark);
        self.store();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(speaker: Speaker, text: &str) -> ChatMessage {
        ChatMessage {
            speaker,
            text: text.to_owned(),
            parts: Vec::new(),
            error: None,
            running: false,
            mentions: Vec::new(),
            checklist: None,
            rewind: None,
        }
    }

    #[test]
    fn an_answer_is_bookmarked_by_the_question_it_answers() {
        let messages = [
            message(Speaker::Assistant, ""),
            message(Speaker::User, "Fix the build"),
            message(Speaker::Assistant, ""),
        ];
        assert_eq!(bookmark_of(&messages, 0), None);
        assert_eq!(
            bookmark_of(&messages, 1),
            Some(Bookmark {
                turn: TurnPrint::from("Fix the build"),
                side: TurnSide::Question,
            })
        );
        assert_eq!(
            bookmark_of(&messages, 2),
            Some(Bookmark {
                turn: TurnPrint::from("Fix the build"),
                side: TurnSide::Answer,
            })
        );
    }
}
