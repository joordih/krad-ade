use gpui_kit::{
    div, px, rgb, AnyElement, Context, IntoElement, ParentElement, ScrollDelta, ScrollWheelEvent,
    Styled, TouchPhase, Window,
};
use std::time::Instant;

use super::EnvironmentShell;
use crate::motion::Spring;
use crate::theme::{palette, row_shadow};

const RESPONSE_S: f32 = 0.32;
const MAX_FRAME_S: f32 = 1.0 / 30.0;
const AXIS_LOCK_PX: f32 = 6.0;
const LEAVE_FRACTION: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Settle {
    Back,
    Away,
}

#[derive(Default)]
pub(super) struct SettingsSwipe {
    tracking: bool,
    travel: (f32, f32),
    axis: Option<Axis>,
    offset: f32,
    width: f32,
    settling: Option<(Spring, Settle, Instant)>,
}

impl SettingsSwipe {
    pub(super) fn offset(&self) -> f32 {
        self.offset.max(0.0)
    }

    fn settle(&mut self, settle: Settle, now: Instant) {
        let mut spring = Spring::at_rest(self.offset, RESPONSE_S);
        spring.set_target(match settle {
            Settle::Back => 0.0,
            Settle::Away => self.width,
        });
        self.tracking = false;
        self.settling = Some((spring, settle, now));
    }

    fn reset(&mut self) {
        *self = Self {
            width: self.width,
            ..Self::default()
        };
    }
}

impl EnvironmentShell {
    pub(super) fn leave_settings(&mut self, cx: &mut Context<Self>) -> bool {
        if self.settings_ui.is_none() {
            return false;
        }
        self.gestures.settings.settle(Settle::Away, Instant::now());
        cx.notify();
        true
    }

    pub(super) fn drag_settings(
        &mut self,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.settings_ui.is_none() {
            return false;
        }
        let ScrollDelta::Pixels(delta) = event.delta else {
            return true;
        };
        let swipe = &mut self.gestures.settings;
        match event.touch_phase {
            TouchPhase::Started => {
                swipe.reset();
                swipe.tracking = true;
            }
            TouchPhase::Moved if swipe.tracking => {
                swipe.travel.0 += delta.x.as_f32();
                swipe.travel.1 += delta.y.as_f32();
                let (dx, dy) = swipe.travel;
                if swipe.axis.is_none() && dx.abs().max(dy.abs()) > AXIS_LOCK_PX {
                    swipe.axis = Some(if dx.abs() > dy.abs() {
                        Axis::Horizontal
                    } else {
                        Axis::Vertical
                    });
                }
                if swipe.axis == Some(Axis::Horizontal) {
                    swipe.offset = dx.max(0.0);
                    cx.notify();
                }
            }
            TouchPhase::Ended if swipe.tracking => {
                let leaving = swipe.axis == Some(Axis::Horizontal)
                    && swipe.offset > swipe.width * LEAVE_FRACTION;
                let settle = if leaving { Settle::Away } else { Settle::Back };
                swipe.settle(settle, Instant::now());
                cx.notify();
            }
            TouchPhase::Cancelled => {
                swipe.settle(Settle::Back, Instant::now());
                cx.notify();
            }
            TouchPhase::Moved | TouchPhase::Ended => {}
        }
        true
    }

    pub(super) fn advance_settings_swipe(
        &mut self,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.gestures.settings.width = window.viewport_size().width.as_f32();
        let Some((mut spring, settle, last)) = self.gestures.settings.settling.take() else {
            return false;
        };
        let elapsed = now.duration_since(last).as_secs_f32().min(MAX_FRAME_S);
        spring.step(elapsed);
        self.gestures.settings.offset = spring.value();
        if !spring.is_settled() {
            self.gestures.settings.settling = Some((spring, settle, now));
            return true;
        }
        self.gestures.settings.reset();
        if settle == Settle::Away {
            self.close_settings(window, cx);
        }
        false
    }

    pub(super) fn sliding_settings(
        &self,
        settings: AnyElement,
        behind: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .child(behind)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(self.gestures.settings.offset()))
                    .w_full()
                    .flex()
                    .bg(rgb(pal.backdrop_from))
                    .shadow(row_shadow(pal, cx))
                    .child(settings),
            )
            .into_any_element()
    }
}
