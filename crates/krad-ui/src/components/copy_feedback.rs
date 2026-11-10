use gpui_kit::{
    div, ease_in_out, Animation, AnimationExt as _, AnyElement, App, AppContext as _,
    ClipboardItem, Context, ElementId, Entity, IntoElement, ParentElement, SharedString, Styled,
};
use std::collections::HashMap;
use std::time::Duration;

use super::{icon, IconName};

const COPY_FEEDBACK_DURATION: Duration = Duration::from_millis(1500);
const COPY_FADE_DURATION: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyGlyph {
    Clipboard,
    Tick,
}

impl From<CopyGlyph> for IconName {
    fn from(glyph: CopyGlyph) -> Self {
        match glyph {
            CopyGlyph::Clipboard => Self::Copy,
            CopyGlyph::Tick => Self::Tick,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CopyPhase {
    Copied,
    Returning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CopyMark {
    press: u64,
    phase: CopyPhase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CopyTicket {
    key: SharedString,
    press: u64,
}

#[derive(Debug, Default)]
pub(crate) struct CopyFeedback {
    marks: HashMap<SharedString, CopyMark>,
    presses: u64,
}

impl CopyFeedback {
    #[must_use]
    pub(crate) fn press(&mut self, key: SharedString) -> CopyTicket {
        self.presses += 1;
        let press = self.presses;
        self.marks.insert(
            key.clone(),
            CopyMark {
                press,
                phase: CopyPhase::Copied,
            },
        );
        CopyTicket { key, press }
    }

    pub(crate) fn start_return(&mut self, ticket: &CopyTicket) -> bool {
        let Some(mark) = self.current_mark(ticket) else {
            return false;
        };
        if mark.phase != CopyPhase::Copied {
            return false;
        }
        mark.phase = CopyPhase::Returning;
        true
    }

    pub(crate) fn finish_return(&mut self, ticket: &CopyTicket) -> bool {
        let finished = self
            .current_mark(ticket)
            .is_some_and(|mark| mark.phase == CopyPhase::Returning);
        if finished {
            self.marks.remove(&ticket.key);
        }
        finished
    }

    pub(crate) fn forget(&mut self, key: &str) {
        self.marks.remove(key);
    }

    #[must_use]
    pub(crate) fn glyph(&self, key: &str) -> CopyGlyph {
        match self.marks.get(key).map(|mark| mark.phase) {
            Some(CopyPhase::Copied) => CopyGlyph::Tick,
            Some(CopyPhase::Returning) | None => CopyGlyph::Clipboard,
        }
    }

    fn fades(&self, key: &str) -> bool {
        self.marks.contains_key(key)
    }

    fn current_mark(&mut self, ticket: &CopyTicket) -> Option<&mut CopyMark> {
        self.marks
            .get_mut(&ticket.key)
            .filter(|mark| mark.press == ticket.press)
    }

    fn show_copied(&mut self, key: SharedString, cx: &mut Context<Self>) {
        let ticket = self.press(key);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPY_FEEDBACK_DURATION).await;
            let returning = this.update(cx, |feedback, cx| {
                let started = feedback.start_return(&ticket);
                if started {
                    cx.notify();
                }
                started
            });
            if !returning.unwrap_or_default() {
                return;
            }
            cx.background_executor().timer(COPY_FADE_DURATION).await;
            this.update(cx, |feedback, cx| {
                if feedback.finish_return(&ticket) {
                    cx.notify();
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }
}

pub(crate) fn copy_with_feedback(
    feedback: &Entity<CopyFeedback>,
    key: SharedString,
    text: String,
    cx: &mut App,
) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    feedback.update(cx, |feedback, cx| feedback.show_copied(key, cx));
}

pub(crate) fn copy_glyph(
    feedback: &Entity<CopyFeedback>,
    key: &SharedString,
    size: f32,
    color: u32,
    cx: &App,
) -> AnyElement {
    let feedback = feedback.read(cx);
    let glyph = feedback.glyph(key);
    let shown = div()
        .flex()
        .flex_none()
        .child(icon(IconName::from(glyph), size, color));
    if !feedback.fades(key) {
        return shown.into_any_element();
    }
    let fade = Animation::new(COPY_FADE_DURATION).with_easing(ease_in_out);
    shown
        .with_animation(
            ElementId::NamedInteger(key.clone(), glyph as u64),
            fade,
            Styled::opacity,
        )
        .into_any_element()
}

pub(crate) fn new_copy_feedback(cx: &mut App) -> Entity<CopyFeedback> {
    cx.new(|_| CopyFeedback::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "copy-answer";

    #[test]
    fn a_press_shows_the_tick_until_its_time_runs_out() {
        let mut feedback = CopyFeedback::default();
        let ticket = feedback.press(KEY.into());
        assert_eq!(feedback.glyph(KEY), CopyGlyph::Tick);
        assert!(feedback.start_return(&ticket));
        assert_eq!(feedback.glyph(KEY), CopyGlyph::Clipboard);
        assert!(feedback.finish_return(&ticket));
        assert_eq!(feedback.glyph(KEY), CopyGlyph::Clipboard);
        assert!(!feedback.fades(KEY));
    }

    #[test]
    fn pressing_again_restarts_the_timer() {
        let mut feedback = CopyFeedback::default();
        let first = feedback.press(KEY.into());
        let second = feedback.press(KEY.into());
        assert!(!feedback.start_return(&first));
        assert_eq!(feedback.glyph(KEY), CopyGlyph::Tick);
        assert!(feedback.start_return(&second));
        assert_eq!(feedback.glyph(KEY), CopyGlyph::Clipboard);
    }

    #[test]
    fn pressing_while_returning_shows_the_tick_again() {
        let mut feedback = CopyFeedback::default();
        let first = feedback.press(KEY.into());
        assert!(feedback.start_return(&first));
        let second = feedback.press(KEY.into());
        assert!(!feedback.finish_return(&first));
        assert_eq!(feedback.glyph(KEY), CopyGlyph::Tick);
        assert!(feedback.start_return(&second));
        assert!(feedback.finish_return(&second));
    }

    #[test]
    fn each_copy_target_keeps_its_own_timer() {
        let mut feedback = CopyFeedback::default();
        let answer = feedback.press(KEY.into());
        let _code = feedback.press("copy-code".into());
        assert!(feedback.start_return(&answer));
        assert_eq!(feedback.glyph(KEY), CopyGlyph::Clipboard);
        assert_eq!(feedback.glyph("copy-code"), CopyGlyph::Tick);
    }

    #[test]
    fn forgetting_a_target_drops_its_pending_timer() {
        let mut feedback = CopyFeedback::default();
        let ticket = feedback.press(KEY.into());
        feedback.forget(KEY);
        assert_eq!(feedback.glyph(KEY), CopyGlyph::Clipboard);
        assert!(!feedback.start_return(&ticket));
    }
}
