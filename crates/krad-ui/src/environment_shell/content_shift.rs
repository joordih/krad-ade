use gpui_kit::{div, px, AnyElement, IntoElement, ParentElement, Styled};
use krad_domain::{EnvironmentView, WorkMode};
use std::time::Instant;

use super::EnvironmentShell;
use crate::motion::{Easing, Tween, LAYOUT_SHIFT};

const ENTERING: f32 = 0.0;
const SETTLED: f32 = 1.0;
const ENTER_OPACITY: f32 = 0.55;
const ENTER_RISE: f32 = 4.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct ShownContent {
    view: EnvironmentView,
    mode: WorkMode,
    filled: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct ContentShift {
    shown: Option<ShownContent>,
    entrance: Tween,
}

impl Default for ContentShift {
    fn default() -> Self {
        Self {
            shown: None,
            entrance: Tween::at_rest(SETTLED),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct ContentLook {
    opacity: f32,
    rise: f32,
}

impl ContentShift {
    pub(super) fn follow(&mut self, content: ShownContent, now: Instant) {
        let before = self.shown.replace(content);
        if before.is_none_or(|before| before == content) {
            return;
        }
        self.entrance = Tween::at_rest(ENTERING);
        self.entrance
            .retarget(SETTLED, LAYOUT_SHIFT, Easing::Emphasized, now);
    }

    pub(super) fn advance(&mut self, now: Instant) -> bool {
        self.entrance.advance(now)
    }

    pub(super) fn settle(&mut self) {
        self.entrance.settle();
    }

    fn look(&self, now: Instant) -> ContentLook {
        let shown = self.entrance.value(now);
        ContentLook {
            opacity: ENTER_OPACITY + (SETTLED - ENTER_OPACITY) * shown,
            rise: ENTER_RISE * (SETTLED - shown),
        }
    }

    pub(super) fn wrap(&self, view: AnyElement, now: Instant) -> AnyElement {
        let look = self.look(now);
        div()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .opacity(look.opacity)
            .top(px(look.rise))
            .child(view)
            .into_any_element()
    }
}

impl EnvironmentShell {
    pub(super) fn shown_content(&self, view: EnvironmentView) -> ShownContent {
        let filled = self.envs.get(&self.active).is_some_and(|live| {
            live.data.current_session().is_some() && live.shown_tab().is_some()
        });
        ShownContent {
            view,
            mode: self.work_mode,
            filled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const SESSIONS: ShownContent = ShownContent {
        view: EnvironmentView::Sessions,
        mode: WorkMode::Ade,
        filled: true,
    };

    fn same(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn the_first_content_appears_without_an_entrance() {
        let now = Instant::now();
        let mut shift = ContentShift::default();
        shift.follow(SESSIONS, now);
        assert!(!shift.advance(now));
        assert!(same(shift.look(now).opacity, SETTLED));
    }

    #[test]
    fn switching_the_work_mode_fades_and_lifts_the_view_in() {
        let now = Instant::now();
        let mut shift = ContentShift::default();
        shift.follow(SESSIONS, now);
        let editor = ShownContent {
            mode: WorkMode::Editor,
            ..SESSIONS
        };
        shift.follow(editor, now);
        let start = shift.look(now);
        assert!(same(start.opacity, ENTER_OPACITY));
        assert!(same(start.rise, ENTER_RISE));
        assert!(shift.advance(now + LAYOUT_SHIFT / 2));
        assert!(!shift.advance(now + LAYOUT_SHIFT));
        let end = shift.look(now + LAYOUT_SHIFT);
        assert!(same(end.opacity, SETTLED));
        assert!(same(end.rise, 0.0));
    }

    #[test]
    fn filling_an_empty_view_or_changing_the_view_starts_an_entrance() {
        let now = Instant::now();
        for next in [
            ShownContent {
                filled: false,
                ..SESSIONS
            },
            ShownContent {
                view: EnvironmentView::Notes,
                ..SESSIONS
            },
        ] {
            let mut shift = ContentShift::default();
            shift.follow(SESSIONS, now);
            shift.follow(next, now);
            assert!(shift.advance(now + Duration::from_millis(1)));
        }
    }

    #[test]
    fn the_same_content_every_frame_keeps_still() {
        let now = Instant::now();
        let mut shift = ContentShift::default();
        shift.follow(SESSIONS, now);
        shift.follow(SESSIONS, now + LAYOUT_SHIFT);
        assert!(!shift.advance(now + LAYOUT_SHIFT));
    }

    #[test]
    fn settling_skips_the_entrance() {
        let now = Instant::now();
        let mut shift = ContentShift::default();
        shift.follow(SESSIONS, now);
        shift.follow(
            ShownContent {
                view: EnvironmentView::Review,
                ..SESSIONS
            },
            now,
        );
        shift.settle();
        assert!(!shift.advance(now));
        assert!(same(shift.look(now).opacity, SETTLED));
    }
}
