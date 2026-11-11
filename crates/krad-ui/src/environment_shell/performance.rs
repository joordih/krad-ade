use gpui_kit::{App, Context, Window};
use krad_domain::PerformanceMode;
use std::time::Instant;

use super::sidebar::PanelReveal;
use super::EnvironmentShell;
use crate::motion::follow_motion_preference;
use crate::theme::advance_theme_fade;

impl EnvironmentShell {
    pub fn apply_performance_mode(&self, cx: &mut App) {
        follow_motion_preference(self.settings.performance_mode.reduces_motion(), cx);
    }

    pub(super) fn set_performance_mode(&mut self, mode: PerformanceMode, cx: &mut Context<Self>) {
        if self.settings.performance_mode == mode {
            return;
        }
        self.settings.performance_mode = mode;
        self.apply_performance_mode(cx);
        self.save_settings(cx);
        cx.notify();
    }

    pub(super) fn advance_motion(
        &mut self,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let moving = advance_theme_fade(now, cx)
            | self.sidebar.advance(now)
            | self.content_shift.advance(now)
            | self.sync_side_panel(now)
            | self.sync_activity(now)
            | self.pane_drag.advance(now)
            | self.sync_chat_landings(now)
            | self.glide_chats(now)
            | self.tick_arcade(now, cx)
            | self.advance_gestures(now, window, cx);
        if moving && cx.reduce_motion() {
            self.settle_reveals();
        }
        moving
    }

    fn settle_reveals(&mut self) {
        self.sidebar.settle();
        self.content_shift.settle();
        self.side_reveal.settle();
        self.activity_reveal.settle();
        self.chat_landings
            .values_mut()
            .for_each(PanelReveal::settle);
    }
}
