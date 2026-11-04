use futures::StreamExt as _;
use gpui_kit::component::input::RopeExt as _;
use gpui_kit::{
    App, Context, Div, InteractiveElement, MouseButton, MouseDownEvent, NavigationDirection,
    PinchEvent, ScrollDelta, ScrollWheelEvent, Task, TouchPhase, Window,
};
use krad_domain::{FileOpening, NavigationPlace, NavigationStep, TerminalId, TextScale};
use krad_lsp::lsp_types::Position;
use std::collections::HashMap;

use super::EnvironmentShell;
use crate::actions::{NavigateBack, NavigateForward};
use crate::native_swipe::{watch_swipes, SwipeDirection};

const SWIPE_MIN_PX: f32 = 160.0;
const JUMP_LINES: u32 = 10;
const SWIPE_DOMINANCE: f32 = 2.5;

#[derive(Default)]
pub(super) struct Gestures {
    pinch: Option<(TerminalId, TextScale)>,
    swipe: (f32, f32),
    native: Option<Task<()>>,
    cursor_lines: HashMap<TerminalId, u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Swipe {
    Left,
    Right,
}

fn swipe_of((dx, dy): (f32, f32)) -> Option<Swipe> {
    if dx.abs() < SWIPE_MIN_PX || dx.abs() < dy.abs() * SWIPE_DOMINANCE {
        return None;
    }
    Some(if dx > 0.0 { Swipe::Right } else { Swipe::Left })
}

impl EnvironmentShell {
    fn text_scale_of(&self, tab: TerminalId) -> Option<TextScale> {
        self.envs
            .get(&self.active)?
            .data
            .nodes
            .iter()
            .find(|node| node.id == tab)
            .map(|node| node.text_scale)
    }

    fn scale_without_storing(&mut self, tab: TerminalId, scale: TextScale, cx: &mut Context<Self>) {
        let env = self.active.clone();
        if let Some(node) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.data.nodes.iter_mut().find(|node| node.id == tab))
        {
            node.text_scale = scale;
        }
        cx.notify();
    }

    pub(super) fn pinch_tab(
        &mut self,
        tab: TerminalId,
        event: &PinchEvent,
        cx: &mut Context<Self>,
    ) {
        match event.phase {
            TouchPhase::Started => {
                self.gestures.pinch = self.text_scale_of(tab).map(|scale| (tab, scale));
            }
            TouchPhase::Moved => {
                if let Some(scale) = self.text_scale_of(tab) {
                    self.scale_without_storing(tab, scale.pinched(event.delta), cx);
                }
            }
            TouchPhase::Ended => {
                self.gestures.pinch = None;
                self.store();
            }
            TouchPhase::Cancelled => {
                if let Some((pinched, start)) = self.gestures.pinch.take() {
                    self.scale_without_storing(pinched, start, cx);
                }
            }
        }
    }
}

impl EnvironmentShell {
    fn current_place(&self, cx: &App) -> Option<NavigationPlace> {
        let live = self.envs.get(&self.active)?;
        let (id, path) = live.shown_file()?;
        let state = live.editors.get(&id)?.state.as_ref()?.read(cx);
        let position = state.text().offset_to_position(state.cursor());
        Some(NavigationPlace {
            path,
            line: position.line,
            column: position.character,
        })
    }

    pub(super) fn track_cursor_jumps(&mut self, cx: &App) {
        let Some(live) = self.envs.get(&self.active) else {
            return;
        };
        let Some((id, path)) = live.shown_file() else {
            return;
        };
        let Some(state) = live
            .editors
            .get(&id)
            .and_then(|editor| editor.state.as_ref())
        else {
            return;
        };
        let state = state.read(cx);
        let cursor_line = state.text().offset_to_position(state.cursor()).line;
        let session = live.data.active_session;
        let Some(before) = self.gestures.cursor_lines.insert(id, cursor_line) else {
            return;
        };
        if before.abs_diff(cursor_line) <= JUMP_LINES {
            return;
        }
        let (Some(session), Some(live)) = (session, self.envs.get_mut(&self.active)) else {
            return;
        };
        live.navigation
            .entry(session)
            .or_default()
            .record(NavigationPlace {
                path,
                line: before,
                column: 0,
            });
    }

    pub(super) fn record_navigation(&mut self, cx: &App) {
        let Some(place) = self.current_place(cx) else {
            return;
        };
        let Some(live) = self.envs.get_mut(&self.active) else {
            return;
        };
        let Some(session) = live.data.active_session else {
            return;
        };
        live.navigation.entry(session).or_default().record(place);
    }

    fn navigate(&mut self, step: NavigationStep, cx: &mut Context<Self>) {
        if step == NavigationStep::Back && self.dismiss_settings(cx) {
            return;
        }
        let current = self.current_place(cx);
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let Some(session) = live.data.active_session else {
            return;
        };
        let Some(place) = live
            .navigation
            .entry(session)
            .or_default()
            .step(step, current)
        else {
            return;
        };
        self.show_file_at_path(&place.path, FileOpening::Pinned, cx);
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        if let Some(id) = live.focused {
            live.pending_cursors
                .insert(id, Position::new(place.line, place.column));
            self.gestures.cursor_lines.insert(id, place.line);
        }
        cx.notify();
    }

    pub(super) fn on_navigate_back(
        &mut self,
        _: &NavigateBack,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate(NavigationStep::Back, cx);
    }

    pub(super) fn on_navigate_forward(
        &mut self,
        _: &NavigateForward,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate(NavigationStep::Forward, cx);
    }

    pub(super) fn on_navigation_button(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let step = match event.button {
            MouseButton::Navigate(NavigationDirection::Back) => NavigationStep::Back,
            MouseButton::Navigate(NavigationDirection::Forward) => NavigationStep::Forward,
            _ => return,
        };
        self.navigate(step, cx);
    }

    pub(super) fn with_gestures(root: Div, cx: &mut Context<Self>) -> Div {
        root.on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Back),
            cx.listener(|this: &mut Self, event: &MouseDownEvent, _, cx| {
                this.on_navigation_button(event, cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Forward),
            cx.listener(|this: &mut Self, event: &MouseDownEvent, _, cx| {
                this.on_navigation_button(event, cx);
            }),
        )
        .on_scroll_wheel(cx.listener(
            |this: &mut Self, event: &ScrollWheelEvent, _, cx| {
                this.on_swipe_scroll(event, cx);
            },
        ))
    }

    pub(super) fn start_watches(&mut self, cx: &mut Context<Self>) {
        self.start_git_watch(cx);
        self.start_swipe_watch(cx);
    }

    fn start_swipe_watch(&mut self, cx: &mut Context<Self>) {
        let Some(mut watch) = watch_swipes() else {
            return;
        };
        self.gestures.native = Some(cx.spawn(async move |this, cx| {
            while let Some(direction) = watch.swipes.next().await {
                let forward = direction == SwipeDirection::Left;
                let swiped = this.update(cx, |shell, cx| {
                    let left_settings = !forward && shell.dismiss_settings(cx);
                    if !left_settings {
                        shell.step_session(forward, cx);
                    }
                });
                if swiped.is_err() {
                    break;
                }
            }
        }));
    }

    pub(super) fn step_session(&mut self, forward: bool, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let stepped = self
            .envs
            .get_mut(&env)
            .is_some_and(|live| live.data.step_session(forward));
        if stepped {
            cx.notify();
            self.store();
        }
    }

    pub(super) fn on_swipe_scroll(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let ScrollDelta::Pixels(delta) = event.delta else {
            return;
        };
        match event.touch_phase {
            TouchPhase::Started | TouchPhase::Cancelled => self.gestures.swipe = (0.0, 0.0),
            TouchPhase::Moved => {
                let (dx, dy) = self.gestures.swipe;
                self.gestures.swipe = (dx + delta.x.as_f32(), dy + delta.y.as_f32());
            }
            TouchPhase::Ended => {
                let swiped = swipe_of(std::mem::take(&mut self.gestures.swipe));
                match (swiped, event.modifiers.alt) {
                    (Some(Swipe::Right), true) => {
                        if !self.dismiss_settings(cx) {
                            self.step_session(false, cx);
                        }
                    }
                    (Some(Swipe::Left), true) => self.step_session(true, cx),
                    (Some(Swipe::Right), false) => self.navigate(NavigationStep::Back, cx),
                    (Some(Swipe::Left), false) => self.navigate(NavigationStep::Forward, cx),
                    (None, _) => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_long_mostly_horizontal_swipe_counts() {
        assert_eq!(swipe_of((200.0, 20.0)), Some(Swipe::Right));
        assert_eq!(swipe_of((-200.0, 10.0)), Some(Swipe::Left));
        assert_eq!(swipe_of((90.0, 0.0)), None);
        assert_eq!(swipe_of((200.0, 120.0)), None);
    }
}
