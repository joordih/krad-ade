use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::{Prompt, TerminalId};
use std::time::{Duration, Instant};

use super::super::chat::question_bubble;
use super::super::EnvironmentShell;
use super::mentions::sent_mentions;
use crate::theme::{palette, LINE_2XS, SPACE_1, SPACE_1_5, TEXT_2XS};

const SENT_TIMEOUT: Duration = Duration::from_secs(15);
const HANDOFF_GRACE: Duration = Duration::from_millis(1500);
const PENDING_OPACITY: f32 = 0.6;
const PENDING_BUBBLE_PREFIX: &str = "surface-pending-question";
const QUEUED_LABEL: &str = "Queued";
const SENDING_LABEL: &str = "Sending…";
const CANCEL_LABEL: &str = "Cancel";
const CANCEL_PREFIX: &str = "surface-cancel";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Queued,
    Sent(Instant),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Outgoing {
    pub(super) prompt: Prompt,
    pub(super) stage: Stage,
}

#[derive(Debug, Default)]
pub(super) struct Outbox {
    pub(super) messages: Vec<Outgoing>,
}

impl Outbox {
    pub(super) fn push(&mut self, prompt: Prompt) {
        self.messages.push(Outgoing {
            prompt,
            stage: Stage::Queued,
        });
    }

    pub(super) fn push_first(&mut self, prompt: Prompt) {
        self.messages.insert(
            0,
            Outgoing {
                prompt,
                stage: Stage::Queued,
            },
        );
    }

    pub(super) fn next_to_send(&mut self, free: bool, now: Instant) -> Option<Prompt> {
        let in_flight = self
            .messages
            .iter()
            .any(|message| matches!(message.stage, Stage::Sent(_)));
        if !free || in_flight {
            return None;
        }
        let next = self
            .messages
            .iter_mut()
            .find(|message| message.stage == Stage::Queued)?;
        next.stage = Stage::Sent(now);
        Some(next.prompt.clone())
    }

    pub(super) fn has_queued(&self) -> bool {
        self.messages
            .iter()
            .any(|message| message.stage == Stage::Queued)
    }

    pub(super) fn delivered(&mut self) {
        if let Some(index) = self
            .messages
            .iter()
            .position(|message| matches!(message.stage, Stage::Sent(_)))
        {
            self.messages.remove(index);
        }
    }

    pub(super) fn settle(&mut self, working: bool, now: Instant) {
        self.messages.retain(|message| match message.stage {
            Stage::Queued => true,
            Stage::Sent(at) => {
                let waited = now.saturating_duration_since(at);
                waited < SENT_TIMEOUT && !(working && waited >= HANDOFF_GRACE)
            }
        });
    }

    pub(super) fn cancel(&mut self, index: usize) -> bool {
        let queued = self
            .messages
            .get(index)
            .is_some_and(|message| message.stage == Stage::Queued);
        if queued {
            self.messages.remove(index);
        }
        queued
    }
}

impl EnvironmentShell {
    pub(super) fn pending_bubble(
        id: TerminalId,
        index: usize,
        outgoing: &Outgoing,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let queued = outgoing.stage == Stage::Queued;
        let raw = u64::try_from(index).unwrap_or(u64::MAX);
        div()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(SPACE_1))
            .opacity(PENDING_OPACITY)
            .when(!outgoing.prompt.text.is_empty(), |bubble| {
                bubble.child(question_bubble(
                    ElementId::Name(
                        format!("{PENDING_BUBBLE_PREFIX}-{}-{raw}", id.as_u64()).into(),
                    ),
                    &outgoing.prompt.text,
                    false,
                    pal,
                ))
            })
            .children(sent_mentions(&outgoing.prompt.mentions, pal))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .text_size(px(TEXT_2XS))
                    .line_height(px(LINE_2XS))
                    .text_color(rgb(pal.text_muted))
                    .child(if queued { QUEUED_LABEL } else { SENDING_LABEL })
                    .when(queued, |line| {
                        line.child(
                            div()
                                .id(ElementId::Name(SharedString::from(format!(
                                    "{CANCEL_PREFIX}-{}-{raw}",
                                    id.as_u64()
                                ))))
                                .cursor_pointer()
                                .text_color(rgb(pal.text_default))
                                .hover(|style| style.text_color(rgb(pal.text_strong)))
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.cancel_queued(id, index, cx);
                                }))
                                .child(CANCEL_LABEL),
                        )
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_waits_while_the_agent_works_and_goes_when_it_is_free() {
        let now = Instant::now();
        let mut outbox = Outbox::default();
        outbox.push(Prompt::from("first"));
        outbox.push(Prompt::from("second"));
        assert_eq!(outbox.next_to_send(false, now), None, "the agent works");
        assert_eq!(outbox.next_to_send(true, now), Some(Prompt::from("first")));
        assert_eq!(outbox.next_to_send(true, now), None, "one at a time");
        outbox.delivered();
        assert_eq!(outbox.next_to_send(true, now), Some(Prompt::from("second")));
    }

    #[test]
    fn only_a_queued_message_can_be_cancelled() {
        let now = Instant::now();
        let mut outbox = Outbox::default();
        outbox.push(Prompt::from("typed"));
        outbox.push(Prompt::from("waiting"));
        outbox.next_to_send(true, now);
        assert!(!outbox.cancel(0), "already handed over");
        assert!(outbox.cancel(1));
        assert_eq!(outbox.messages.len(), 1);
    }

    #[test]
    fn a_handed_over_message_the_conversation_never_shows_is_let_go() {
        let now = Instant::now();
        let mut outbox = Outbox::default();
        outbox.push(Prompt::from("/clear"));
        outbox.next_to_send(true, now);
        outbox.settle(false, now + HANDOFF_GRACE);
        assert_eq!(outbox.messages.len(), 1, "an idle agent may still take it");
        outbox.settle(true, now + HANDOFF_GRACE);
        assert!(outbox.messages.is_empty(), "the agent works on it");
        outbox.push(Prompt::from("late"));
        outbox.next_to_send(true, now);
        outbox.settle(false, now + SENT_TIMEOUT);
        assert_eq!(outbox.messages, Vec::new());
    }
}
