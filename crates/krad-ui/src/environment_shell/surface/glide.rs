use gpui_kit::px;
use std::time::Instant;

use super::super::EnvironmentShell;
use super::SurfaceChat;
use crate::motion::Spring;

const GLIDE_RESPONSE_S: f32 = 0.35;

#[derive(Clone, Copy, Debug)]
pub(super) struct Glide {
    spring: Spring,
    last: Instant,
}

impl SurfaceChat {
    fn scrolled(&self) -> f32 {
        -self.thread.scroll_px_offset_for_scrollbar().y.as_f32()
    }

    pub(super) fn start_glide(&mut self, now: Instant) {
        self.thread.pause_following_tail();
        self.glide = Some(Glide {
            spring: Spring::at_rest(self.scrolled(), GLIDE_RESPONSE_S),
            last: now,
        });
    }

    fn advance_glide(&mut self, now: Instant) -> bool {
        let scrolled = self.scrolled();
        let Some(glide) = self.glide.as_mut() else {
            return false;
        };
        glide
            .spring
            .set_target(self.thread.max_offset_for_scrollbar().y.as_f32());
        glide.spring.step((now - glide.last).as_secs_f32());
        glide.last = now;
        self.thread.scroll_by(px(glide.spring.value() - scrolled));
        if !glide.spring.is_settled() {
            return true;
        }
        self.glide = None;
        self.thread.scroll_to_end();
        false
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn glide_chats(&mut self, now: Instant) -> bool {
        let Some(live) = self.envs.get_mut(&self.active) else {
            return false;
        };
        live.chats
            .values_mut()
            .fold(false, |moving, chat| chat.advance_glide(now) | moving)
    }
}
