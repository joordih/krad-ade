use futures::StreamExt as _;
use gpui_kit::component::input::RopeExt as _;
use gpui_kit::{
    canvas, AnyElement, App, Context, DispatchPhase, Div, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, NavigationDirection, ParentElement, ScrollEvent, Styled, Task,
    TouchPhase, Window,
};
use krad_domain::{FileOpening, NavigationPlace, NavigationStep, TerminalId};
use krad_lsp::lsp_types::Position;
use std::collections::HashMap;

use super::settings_swipe::SettingsSwipe;
use super::EnvironmentShell;
use crate::actions::{NavigateBack, NavigateForward};
use crate::native_swipe::watch_swipes;

const JUMP_LINES: u32 = 10;

#[derive(Default)]
pub(super) struct Gestures {
    native: Option<Task<()>>,
    cursor_lines: HashMap<TerminalId, u32>,
    pub(super) settings: SettingsSwipe,
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
        if step == NavigationStep::Back && self.leave_settings(cx) {
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
        .on_scroll(cx.listener(|this: &mut Self, event: &ScrollEvent, _, cx| {
            if event.touch_phase == TouchPhase::Moved {
                this.drag_settings(event, cx);
            }
        }))
        .child(Self::swipe_bounds_watch(cx))
    }

    fn swipe_bounds_watch(cx: &mut Context<Self>) -> AnyElement {
        let shell = cx.entity().downgrade();
        canvas(
            |_, _, _| (),
            move |_, (), window, _| {
                let shell = shell.clone();
                window.on_mouse_event(move |event: &ScrollEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture || event.touch_phase == TouchPhase::Moved {
                        return;
                    }
                    shell
                        .update(cx, |shell, cx| shell.drag_settings(event, cx))
                        .unwrap_or_default();
                });
            },
        )
        .absolute()
        .size_0()
        .into_any_element()
    }

    pub(super) fn advance_gestures(
        &mut self,
        now: std::time::Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.advance_settings_swipe(now, window, cx)
    }

    pub(super) fn start_watches(&mut self, cx: &mut Context<Self>) {
        self.start_group_checks(cx);
        self.start_swipe_watch(cx);
    }

    fn start_swipe_watch(&mut self, cx: &mut Context<Self>) {
        let Some(mut watch) = watch_swipes() else {
            return;
        };
        self.gestures.native = Some(cx.spawn(async move |this, cx| {
            while let Some(direction) = watch.swipes.next().await {
                let swiped = this.update(cx, |shell, cx| {
                    if direction.is_back() {
                        shell.leave_settings(cx);
                    }
                });
                if swiped.is_err() {
                    break;
                }
            }
        }));
    }
}
