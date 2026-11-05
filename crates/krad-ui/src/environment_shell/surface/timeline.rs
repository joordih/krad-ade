use gpui_kit::{
    div, px, relative, rgb, AnyElement, Context, InteractiveElement, IntoElement, ListOffset,
    ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_chat::Speaker;
use krad_domain::{Bookmark, ChatBookmarks, TerminalId};
use std::cell::Ref;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use super::super::chat::{AnswerPart, ChatMessage};
use super::super::EnvironmentShell;
use super::bookmarks::bookmarks_of;
use super::thread::first_message_row;
use super::SurfaceChat;
use crate::components::tooltip_view;
use crate::theme::{palette, Palette, SPACE_1, SPACE_2};

const MIN_QUESTIONS: usize = 2;
const LABEL_CHARS: usize = 80;
const ELLIPSIS: char = '…';
const STRIP_W: f32 = 14.0;
const STRIP_INSET: f32 = SPACE_1;
const STRIP_PAD_Y: f32 = SPACE_2;
const TICK_HIT_H: f32 = 8.0;
const TICK_H: f32 = 2.0;
const QUESTION_TICK_W: f32 = 8.0;
const BOOKMARK_TICK_W: f32 = 12.0;
const TIMELINE_PREFIX: &str = "surface-timeline";
const BOOKMARK_LABEL: &str = "Bookmark: ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TickKind {
    Question,
    Bookmark,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Tick {
    pub(super) kind: TickKind,
    pub(super) at: f32,
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
    marks: Vec<Option<Bookmark>>,
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

#[allow(clippy::cast_precision_loss)]
fn position(row: usize, rows: usize) -> f32 {
    if rows == 0 {
        return 0.0;
    }
    (row as f32 / rows as f32).clamp(0.0, 1.0)
}

pub(super) fn ticks<'a>(turns: impl IntoIterator<Item = TurnRow<'a>>, rows: usize) -> Vec<Tick> {
    let mut ticks = Vec::new();
    let mut unanswered: Option<usize> = None;
    for turn in turns {
        match turn.speaker {
            Speaker::User => {
                unanswered = Some(ticks.len());
                ticks.push(Tick {
                    kind: TickKind::Question,
                    at: position(turn.row, rows),
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
                at: position(turn.row, rows),
                row: turn.row,
                target: turn.row,
                label: format!("{BOOKMARK_LABEL}{}", label(turn.text)).into(),
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

fn timeline_print(chat: &SurfaceChat, bookmarks: &ChatBookmarks) -> u64 {
    let mut hasher = DefaultHasher::new();
    chat.thread.item_count().hash(&mut hasher);
    first_message_row(chat).hash(&mut hasher);
    chat.messages.len().hash(&mut hasher);
    for message in &chat.messages {
        message.text.len().hash(&mut hasher);
        message.parts.len().hash(&mut hasher);
    }
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
    ticks(turns, chat.thread.item_count())
}

impl SurfaceChat {
    fn refreshed_timeline(&self, bookmarks: &ChatBookmarks) -> Ref<'_, Timeline> {
        let print = timeline_print(self, bookmarks);
        if self.timeline.borrow().print != print {
            let mut timeline = self.timeline.borrow_mut();
            timeline.marks = bookmarks_of(&self.messages);
            timeline.ticks = chat_ticks(self, &timeline.marks, bookmarks).into();
            timeline.print = print;
        }
        self.timeline.borrow()
    }

    fn timeline_ticks(&self, bookmarks: &ChatBookmarks) -> Rc<[Tick]> {
        self.refreshed_timeline(bookmarks).ticks.clone()
    }

    pub(super) fn message_bookmark(
        &self,
        index: usize,
        bookmarks: &ChatBookmarks,
    ) -> Option<Bookmark> {
        self.refreshed_timeline(bookmarks)
            .marks
            .get(index)
            .copied()
            .flatten()
    }

    fn jump_to_row(&mut self, row: usize) {
        self.glide = None;
        self.thread.scroll_to(ListOffset {
            item_ix: row,
            offset_in_item: px(0.0),
        });
    }
}

fn tick_color(kind: TickKind, current: bool, pal: &Palette) -> u32 {
    match kind {
        TickKind::Bookmark => pal.attention,
        TickKind::Question if current => pal.accent,
        TickKind::Question => pal.text_disabled,
    }
}

impl EnvironmentShell {
    pub(super) fn timeline_strip(
        &self,
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let empty = ChatBookmarks::default();
        let bookmarks = self.chat_bookmarks(id).unwrap_or(&empty);
        let ticks = chat.timeline_ticks(bookmarks);
        let questions = ticks
            .iter()
            .filter(|tick| tick.kind == TickKind::Question)
            .count();
        if questions < MIN_QUESTIONS {
            return None;
        }
        let current = current_question(&ticks, chat.thread.logical_scroll_top().item_ix);
        let pal = palette(cx);
        let marks = ticks.iter().enumerate().map(|(index, tick)| {
            let target = tick.target;
            let tooltip = tick.label.clone();
            let width = match tick.kind {
                TickKind::Question => QUESTION_TICK_W,
                TickKind::Bookmark => BOOKMARK_TICK_W,
            };
            let color = tick_color(tick.kind, current == Some(index), pal);
            div()
                .id(index)
                .absolute()
                .right_0()
                .top(relative(tick.at))
                .mt(px(-TICK_HIT_H / 2.0))
                .h(px(TICK_HIT_H))
                .w(px(STRIP_W))
                .flex()
                .items_center()
                .justify_end()
                .cursor_pointer()
                .tooltip(move |_, cx| tooltip_view(tooltip.clone(), cx))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.jump_on_timeline(id, target);
                    cx.notify();
                }))
                .child(
                    div()
                        .h(px(TICK_H))
                        .w(px(width))
                        .rounded_full()
                        .bg(rgb(color)),
                )
        });
        Some(
            div()
                .id((TIMELINE_PREFIX, id.as_u64()))
                .absolute()
                .top(px(STRIP_PAD_Y))
                .bottom(px(STRIP_PAD_Y))
                .right(px(STRIP_INSET))
                .w(px(STRIP_W))
                .children(marks)
                .into_any_element(),
        )
    }

    fn jump_on_timeline(&mut self, id: TerminalId, row: usize) {
        if let Some(chat) = self
            .envs
            .get_mut(&self.active)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.jump_to_row(row);
        }
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
    fn each_question_gets_a_tick_where_its_turn_sits_in_the_transcript() {
        let ticks = ticks(conversation(), 8);
        let questions: Vec<(usize, f32)> = ticks
            .iter()
            .filter(|tick| tick.kind == TickKind::Question)
            .map(|tick| (tick.row, tick.at))
            .collect();
        assert_eq!(questions, vec![(1, 0.125), (3, 0.375)]);
    }

    #[test]
    fn ticks_move_up_as_the_transcript_grows() {
        let short = ticks(conversation(), 8);
        let longer = ticks(conversation(), 16);
        assert!(longer[1].at < short[1].at);
    }

    #[test]
    fn a_question_tick_lands_on_its_answer() {
        let ticks = ticks(conversation(), 8);
        let targets: Vec<usize> = ticks
            .iter()
            .filter(|tick| tick.kind == TickKind::Question)
            .map(|tick| tick.target)
            .collect();
        assert_eq!(targets, vec![2, 4]);
    }

    #[test]
    fn a_question_still_unanswered_lands_on_itself() {
        let ticks = ticks([turn(5, Speaker::User, "Anything else?", false)], 6);
        assert_eq!(ticks[0].target, 5);
    }

    #[test]
    fn a_bookmark_gets_its_own_tick_on_its_message() {
        let ticks = ticks(conversation(), 8);
        let bookmark = ticks
            .iter()
            .find(|tick| tick.kind == TickKind::Bookmark)
            .map(|tick| (tick.row, tick.target, tick.label.to_string()));
        assert_eq!(
            bookmark,
            Some((4, 4, format!("{BOOKMARK_LABEL}They pass too.")))
        );
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
        let ticks = ticks(conversation(), 8);
        assert_eq!(current_question(&ticks, 0), None);
        assert_eq!(current_question(&ticks, 2), Some(0));
        assert_eq!(current_question(&ticks, 4), Some(1));
    }
}
