use gpui_kit::{FollowMode, ListState};
use krad_domain::TerminalId;

use super::super::EnvironmentShell;
use super::view::following;
use super::SurfaceChat;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Anchor {
    #[default]
    Tail,
    Question,
}

pub(super) fn follows_tail(anchor: Anchor, thread: &ListState) -> bool {
    anchor == Anchor::Tail && following(thread)
}

fn glide_target(max: f32, landing: Option<f32>) -> f32 {
    landing.map_or(max, |landing| landing.clamp(0.0, max.max(0.0)))
}

fn landing_offset(scrolled: f32, question_from_view: f32, lead: f32) -> f32 {
    scrolled + question_from_view - lead
}

impl SurfaceChat {
    pub(super) fn scrolled(&self) -> f32 {
        -self.thread.scroll_px_offset_for_scrollbar().y.as_f32()
    }

    pub(super) fn anchor_question(&mut self) {
        self.anchor = Anchor::Question;
        self.landing.set(None);
        self.thread.set_follow_mode(FollowMode::Normal);
    }

    pub(super) fn glide_target(&self) -> f32 {
        glide_target(
            self.thread.max_offset_for_scrollbar().y.as_f32(),
            self.landing.get(),
        )
    }

    pub(super) fn note_landing(&self, question: usize, lead: f32) {
        if self.anchor != Anchor::Question {
            return;
        }
        let Some(bounds) = self.thread.bounds_for_item(question) else {
            return;
        };
        let view_top = self.thread.viewport_bounds().origin.y;
        let from_view = (bounds.origin.y - view_top).as_f32();
        self.landing
            .set(Some(landing_offset(self.scrolled(), from_view, lead)));
    }

    fn release_anchor(&mut self) {
        if self.anchor == Anchor::Tail {
            return;
        }
        self.anchor = Anchor::Tail;
        self.glide = None;
        let kept = self.thread.logical_scroll_top();
        self.thread.set_follow_mode(FollowMode::Tail);
        self.thread.scroll_to(kept);
    }

    fn follow_latest(&mut self) {
        self.anchor = Anchor::Tail;
        self.glide = None;
        self.thread.set_follow_mode(FollowMode::Tail);
    }
}

impl EnvironmentShell {
    fn active_chat_mut(&mut self, id: TerminalId) -> Option<&mut SurfaceChat> {
        self.envs.get_mut(&self.active)?.chats.get_mut(&id)
    }

    pub(super) fn scrolled_by_hand(&mut self, id: TerminalId) {
        if let Some(chat) = self.active_chat_mut(id) {
            chat.release_anchor();
        }
    }

    pub(super) fn jump_to_latest(&mut self, id: TerminalId) {
        if let Some(chat) = self.active_chat_mut(id) {
            chat.follow_latest();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_glide_stops_where_the_question_reaches_the_top() {
        assert!((glide_target(900.0, Some(400.0)) - 400.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_glide_cannot_go_past_the_end_or_above_the_start() {
        assert!((glide_target(300.0, Some(400.0)) - 300.0).abs() < f32::EPSILON);
        assert!(glide_target(300.0, Some(-20.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn the_glide_heads_for_the_end_until_the_question_is_measured() {
        assert!((glide_target(900.0, None) - 900.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_landing_leaves_the_lead_above_the_question() {
        let landing = landing_offset(1000.0, 350.0, 20.0);
        assert!((landing - 1330.0).abs() < f32::EPSILON);
    }

    #[test]
    fn an_anchored_question_never_follows_the_tail() {
        let thread = ListState::new(0, gpui_kit::ListAlignment::Top, gpui_kit::px(0.0));
        thread.set_follow_mode(FollowMode::Tail);
        assert!(follows_tail(Anchor::Tail, &thread));
        assert!(!follows_tail(Anchor::Question, &thread));
    }
}
