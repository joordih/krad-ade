use gpui_kit::{Context, ElementId, IntoElement, StatefulInteractiveElement};
use krad_chat::Speaker;
use krad_domain::{Bookmark, ChatBookmarks, Mention, TerminalId, TurnPrint, TurnSide};
use std::collections::HashMap;

use super::super::chat::ChatMessage;
use super::super::EnvironmentShell;
use super::message_actions::{tinted_action_button, BookmarkAction};
use super::SurfaceChat;
use crate::components::IconName;
use crate::theme::Palette;

const BOOKMARK_PREFIX: &str = "surface-bookmark";
const ADD_TOOLTIP: &str = "Bookmark message";
const REMOVE_TOOLTIP: &str = "Remove bookmark";

fn question_print(question: &ChatMessage) -> Option<TurnPrint> {
    let text = question.text.trim();
    let images = question
        .mentions
        .iter()
        .filter_map(|mention| match mention {
            Mention::Image(image) => Some(image.path.as_os_str().as_encoded_bytes()),
            Mention::Path(_) => None,
        });
    let mut print = TurnPrint::from(text);
    let mut shown = !text.is_empty();
    for image in images {
        print = print.and(image);
        shown = true;
    }
    shown.then_some(print)
}

pub(super) fn bookmarks_of(messages: &[ChatMessage]) -> Vec<Option<Bookmark>> {
    let mut asked: HashMap<TurnPrint, u32> = HashMap::new();
    let mut turn: Option<(TurnPrint, u32)> = None;
    messages
        .iter()
        .map(|message| {
            let side = match message.speaker {
                Speaker::User => {
                    turn = question_print(message).map(|print| {
                        let seen = asked.entry(print).or_default();
                        let repeat = *seen;
                        *seen += 1;
                        (print, repeat)
                    });
                    TurnSide::Question
                }
                Speaker::Assistant => TurnSide::Answer,
            };
            turn.map(|(print, repeat)| Bookmark {
                turn: print,
                repeat,
                side,
            })
        })
        .collect()
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
        chat: &SurfaceChat,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Option<BookmarkAction> {
        let empty = ChatBookmarks::default();
        let bookmarks = self.chat_bookmarks(id).unwrap_or(&empty);
        let bookmark = chat.message_bookmark(index)?;
        let marked = bookmarks.contains(&bookmark);
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
    use krad_domain::{ChatImage, ImageFormat};
    use std::path::PathBuf;

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

    fn picture(path: &str) -> ChatMessage {
        let mut question = message(Speaker::User, "");
        question.mentions.push(Mention::Image(ChatImage {
            path: PathBuf::from(path),
            format: ImageFormat::Png,
        }));
        question
    }

    #[test]
    fn an_answer_is_bookmarked_with_the_question_it_answers() {
        let marks = bookmarks_of(&[
            message(Speaker::Assistant, ""),
            message(Speaker::User, "Fix the build"),
            message(Speaker::Assistant, ""),
        ]);
        let turn = TurnPrint::from("Fix the build");
        assert_eq!(marks[0], None);
        assert_eq!(
            marks[1],
            Some(Bookmark {
                turn,
                repeat: 0,
                side: TurnSide::Question,
            })
        );
        assert_eq!(
            marks[2],
            Some(Bookmark {
                turn,
                repeat: 0,
                side: TurnSide::Answer,
            })
        );
    }

    #[test]
    fn the_same_question_asked_twice_gets_two_bookmarks() {
        let marks = bookmarks_of(&[
            message(Speaker::User, "Run the tests"),
            message(Speaker::Assistant, ""),
            message(Speaker::User, "Run the tests"),
            message(Speaker::Assistant, ""),
        ]);
        assert_ne!(marks[1], marks[3]);
        assert_eq!(marks[3].map(|mark| mark.repeat), Some(1));
    }

    #[test]
    fn a_question_with_only_an_image_can_be_bookmarked_by_its_image() {
        let marks = bookmarks_of(&[picture("/shots/a.png"), picture("/shots/b.png")]);
        assert!(marks[0].is_some());
        assert_ne!(marks[0], marks[1]);
        assert_eq!(marks[1].map(|mark| mark.repeat), Some(0));
    }
}
