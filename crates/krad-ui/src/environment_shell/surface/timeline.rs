use gpui_kit::{px, ListOffset, SharedString};
use krad_chat::Speaker;
use krad_domain::{Bookmark, ChatBookmarks};
use std::cell::Ref;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use super::super::chat::{AnswerPart, ChatMessage};
use super::bookmarks::bookmarks_of;
use super::thread::first_message_row;
use super::SurfaceChat;

mod strip;

const LABEL_CHARS: usize = 80;
const ELLIPSIS: char = '…';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TickKind {
    Question,
    Bookmark,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Tick {
    pub(super) kind: TickKind,
    pub(super) row: usize,
    pub(super) target: usize,
    pub(super) label: SharedString,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TurnRow<'a> {
    pub(super) row: usize,
    pub(super) speaker: Speaker,
    pub(super) text: &'a str,
    pub(super) bookmarked: bool,
}

#[derive(Default)]
pub(super) struct Timeline {
    print: u64,
    ticks: Rc<[Tick]>,
    marks_print: u64,
    marks: Vec<Option<Bookmark>>,
    open: bool,
}

fn label(text: &str) -> SharedString {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= LABEL_CHARS {
        return line.into();
    }
    let mut cut: String = line.chars().take(LABEL_CHARS).collect();
    cut.push(ELLIPSIS);
    cut.into()
}

pub(super) fn ticks<'a>(turns: impl IntoIterator<Item = TurnRow<'a>>) -> Vec<Tick> {
    let mut ticks = Vec::new();
    let mut unanswered: Option<usize> = None;
    for turn in turns {
        match turn.speaker {
            Speaker::User => {
                unanswered = Some(ticks.len());
                ticks.push(Tick {
                    kind: TickKind::Question,
                    row: turn.row,
                    target: turn.row,
                    label: label(turn.text),
                });
            }
            Speaker::Assistant => {
                if let Some(question) = unanswered.take().and_then(|tick| ticks.get_mut(tick)) {
                    question.target = turn.row;
                }
            }
        }
        if turn.bookmarked {
            ticks.push(Tick {
                kind: TickKind::Bookmark,
                row: turn.row,
                target: turn.row,
                label: label(turn.text),
            });
        }
    }
    ticks
}

pub(super) fn current_question(ticks: &[Tick], top: usize) -> Option<usize> {
    ticks
        .iter()
        .rposition(|tick| tick.kind == TickKind::Question && tick.row <= top)
}

fn shown_text(message: &ChatMessage) -> &str {
    if message.speaker == Speaker::User {
        return &message.text;
    }
    message
        .parts
        .iter()
        .find_map(|part| match part {
            AnswerPart::Text(text) => Some(text.as_str()),
            AnswerPart::Thinking(_) | AnswerPart::Tool(_) | AnswerPart::Ask(_) => None,
        })
        .unwrap_or(&message.text)
}

fn messages_print(messages: &[ChatMessage]) -> u64 {
    let mut hasher = DefaultHasher::new();
    messages.len().hash(&mut hasher);
    for message in messages {
        message.text.len().hash(&mut hasher);
        message.parts.len().hash(&mut hasher);
        message.mentions.len().hash(&mut hasher);
    }
    hasher.finish()
}

fn timeline_print(chat: &SurfaceChat, marks_print: u64, bookmarks: &ChatBookmarks) -> u64 {
    let mut hasher = DefaultHasher::new();
    chat.thread.item_count().hash(&mut hasher);
    first_message_row(chat).hash(&mut hasher);
    marks_print.hash(&mut hasher);
    bookmarks.hash(&mut hasher);
    hasher.finish()
}

fn chat_ticks(
    chat: &SurfaceChat,
    marks: &[Option<Bookmark>],
    bookmarks: &ChatBookmarks,
) -> Vec<Tick> {
    let first = first_message_row(chat);
    let turns = chat
        .messages
        .iter()
        .enumerate()
        .map(|(index, message)| TurnRow {
            row: first + index,
            speaker: message.speaker,
            text: shown_text(message),
            bookmarked: marks
                .get(index)
                .copied()
                .flatten()
                .is_some_and(|bookmark| bookmarks.contains(&bookmark)),
        });
    ticks(turns)
}

impl SurfaceChat {
    fn refreshed_marks(&self) -> Ref<'_, Timeline> {
        let print = messages_print(&self.messages);
        if self.timeline.borrow().marks_print != print {
            let mut timeline = self.timeline.borrow_mut();
            timeline.marks = bookmarks_of(&self.messages);
            timeline.marks_print = print;
        }
        self.timeline.borrow()
    }

    fn timeline_ticks(&self, bookmarks: &ChatBookmarks) -> Rc<[Tick]> {
        let marks_print = self.refreshed_marks().marks_print;
        let print = timeline_print(self, marks_print, bookmarks);
        if self.timeline.borrow().print != print {
            let mut timeline = self.timeline.borrow_mut();
            timeline.ticks = chat_ticks(self, &timeline.marks, bookmarks).into();
            timeline.print = print;
        }
        self.timeline.borrow().ticks.clone()
    }

    pub(super) fn message_bookmark(&self, index: usize) -> Option<Bookmark> {
        self.refreshed_marks().marks.get(index).copied().flatten()
    }

    fn jump_to_row(&mut self, row: usize) {
        self.glide = None;
        self.thread.scroll_to(ListOffset {
            item_ix: row,
            offset_in_item: px(0.0),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(row: usize, speaker: Speaker, text: &str, bookmarked: bool) -> TurnRow<'_> {
        TurnRow {
            row,
            speaker,
            text,
            bookmarked,
        }
    }

    fn conversation() -> Vec<TurnRow<'static>> {
        vec![
            turn(1, Speaker::User, "Fix the build", false),
            turn(2, Speaker::Assistant, "Done, the build passes.", false),
            turn(3, Speaker::User, "Now the tests", false),
            turn(4, Speaker::Assistant, "They pass too.", true),
        ]
    }

    #[test]
    fn each_question_gets_a_tick_in_the_order_it_was_asked() {
        let rows: Vec<usize> = ticks(conversation())
            .iter()
            .filter(|tick| tick.kind == TickKind::Question)
            .map(|tick| tick.row)
            .collect();
        assert_eq!(rows, vec![1, 3]);
    }

    #[test]
    fn a_question_tick_lands_on_its_answer() {
        let ticks = ticks(conversation());
        let targets: Vec<usize> = ticks
            .iter()
            .filter(|tick| tick.kind == TickKind::Question)
            .map(|tick| tick.target)
            .collect();
        assert_eq!(targets, vec![2, 4]);
    }

    #[test]
    fn a_question_still_unanswered_lands_on_itself() {
        let ticks = ticks([turn(5, Speaker::User, "Anything else?", false)]);
        assert_eq!(ticks[0].target, 5);
    }

    #[test]
    fn a_bookmark_gets_its_own_tick_on_its_message() {
        let ticks = ticks(conversation());
        let bookmark = ticks
            .iter()
            .find(|tick| tick.kind == TickKind::Bookmark)
            .map(|tick| (tick.row, tick.target, tick.label.to_string()));
        assert_eq!(bookmark, Some((4, 4, "They pass too.".to_owned())));
    }

    #[test]
    fn the_tooltip_shows_the_start_of_the_question_on_one_line() {
        let long = format!("Fix\nthe   {}", "x".repeat(LABEL_CHARS));
        let shown = label(&long);
        assert!(shown.starts_with("Fix the x"));
        assert!(shown.ends_with(ELLIPSIS));
        assert_eq!(shown.chars().count(), LABEL_CHARS + 1);
    }

    #[test]
    fn the_turn_in_view_is_the_last_question_at_or_above_the_top() {
        let ticks = ticks(conversation());
        assert_eq!(current_question(&ticks, 0), None);
        assert_eq!(current_question(&ticks, 2), Some(0));
        assert_eq!(current_question(&ticks, 4), Some(1));
    }
}
