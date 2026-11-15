use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, font, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, ScrollWheelEvent, SharedString, Styled, Window,
};
use krad_domain::TerminalId;
use krad_runtime::TerminalAction;
use krad_session::{CellPoint, MouseAction};
use std::time::Duration;

use super::drag::DragState;
use super::live_terminal::LiveTerminal;
use super::surface::elapsed_label;
use super::terminal_images::TerminalImages;
use super::terminal_text::link_at;
use super::EnvironmentShell;

mod geometry;
mod screen;

use crate::components::thinking_indicator;
use crate::terminal_theme::TerminalColors;
use crate::theme::{font_features, mono_family, palette, terminal_fill, TERMINAL_FONT_SIZE};
pub(super) use geometry::{
    body_grid, BodyGeometry, BODY_PAD_BOTTOM, BODY_PAD_TOP, BODY_PAD_X, TERMINAL_FONT_PX,
    TERMINAL_LINE_PX,
};
use screen::session_screen;
pub(super) use screen::{embedded_screen, BodyLook};

const GRID_PROBE_CHAR: char = 'm';
const CARD_STARTING_TEXT: &str = "starting…";
const AGENT_STARTING_PREFIX: &str = "Starting";
const AGENT_STARTING_SUFFIX: &str = "…";
const AGENT_FALLBACK_NAME: &str = "the agent";
const AGENT_START_LIMIT: Duration = Duration::from_secs(20);

fn agent_starting(session: &LiveTerminal) -> bool {
    session.is_agent()
        && session.spawned_at.elapsed() < AGENT_START_LIMIT
        && session.is_running()
        && !session.hooked()
}

fn starting_view(
    id: TerminalId,
    session: &LiveTerminal,
    mono: &SharedString,
    cx: &mut Context<EnvironmentShell>,
) -> AnyElement {
    let name = session
        .harness
        .map_or(AGENT_FALLBACK_NAME, krad_domain::AgentPreset::display_name);
    let label = format!("{AGENT_STARTING_PREFIX} {name}{AGENT_STARTING_SUFFIX}");
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .child(thinking_indicator(
            id.as_u64(),
            label.into(),
            elapsed_label(session.spawned_at.elapsed()).into(),
            mono.clone(),
            palette(cx),
        ))
        .into_any_element()
}
const STARTING_OPACITY: f32 = 0.6;

impl EnvironmentShell {
    pub(super) fn terminal_body(
        id: TerminalId,
        session: Option<&LiveTerminal>,
        uploaded: Option<&TerminalImages>,
        look: BodyLook,
        mono: &SharedString,
        colors: &TerminalColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let BodyLook { geometry, zoom, .. } = look;
        let body = div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .px(px(BODY_PAD_X * zoom))
            .pt(px(BODY_PAD_TOP * zoom))
            .pb(px(BODY_PAD_BOTTOM * zoom))
            .rounded_b(px(look.corner))
            .when_some(
                terminal_fill(colors.background, palette(cx), cx),
                Styled::bg,
            )
            .font_family(mono.clone())
            .font_features(font_features(look.ligatures))
            .text_size(px(TERMINAL_FONT_PX * look.text_zoom))
            .line_height(px(geometry.line_h))
            .text_color(rgb(colors.foreground))
            .whitespace_nowrap()
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this: &mut Self, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.focus_pane(window, cx);
                    this.press_terminal(id, geometry, event, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this: &mut Self, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.focus_pane(window, cx);
                    this.paste_into(id, cx);
                }),
            )
            .on_scroll_wheel(cx.listener(
                move |this: &mut Self, event: &ScrollWheelEvent, _, cx| {
                    cx.stop_propagation();
                    if event.modifiers.alt {
                        this.on_swipe_scroll(event, cx);
                        return;
                    }
                    let delta = event.delta.pixel_delta(px(geometry.line_h));
                    if event.modifiers.secondary() {
                        this.step_text_scale(id, delta.y.as_f32() < 0.0, cx);
                    } else {
                        let cell =
                            geometry.cell_at(event.position.x.as_f32(), event.position.y.as_f32());
                        this.scroll_terminal(id, delta.y.as_f32() / geometry.line_h, cell, cx);
                    }
                },
            ));
        match session {
            Some(session) if agent_starting(session) => body
                .child(starting_view(id, session, mono, cx))
                .into_any_element(),
            Some(session) => session_screen(body, session, uploaded, look, mono, colors),
            None => body
                .opacity(STARTING_OPACITY)
                .child(CARD_STARTING_TEXT)
                .into_any_element(),
        }
    }

    fn press_terminal(
        &mut self,
        id: TerminalId,
        geometry: BodyGeometry,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let cell = geometry.cell_at(event.position.x.as_f32(), event.position.y.as_f32());
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        live.focused = Some(id);
        let Some(session) = live.sessions.get(&id) else {
            cx.notify();
            return;
        };
        if event.modifiers.secondary() {
            if let Some(url) = link_at(&session.view.frame().screen.rows, cell) {
                cx.open_url(&url);
                return;
            }
        }
        let report = !event.modifiers.shift && session.view.status().mouse_tracking;
        self.act_on_terminal(&env, id, TerminalAction::Press { at: cell, report });
        self.drag = if report {
            DragState::Report { id, geometry, cell }
        } else {
            DragState::Select { id, geometry }
        };
        cx.notify();
    }

    pub(super) fn report_click(&mut self, id: TerminalId, action: MouseAction, cell: CellPoint) {
        let env = self.active.clone();
        self.act_on_terminal(&env, id, TerminalAction::Mouse { action, at: cell });
    }

    fn scroll_terminal(
        &mut self,
        id: TerminalId,
        lines: f32,
        cell: CellPoint,
        cx: &mut Context<Self>,
    ) {
        self.scroll_remainder += lines;
        let whole = self.scroll_remainder.trunc();
        if whole == 0.0 {
            return;
        }
        self.scroll_remainder -= whole;
        #[allow(clippy::cast_possible_truncation)]
        let steps = whole as i32;
        let env = self.active.clone();
        self.act_on_terminal(
            &env,
            id,
            TerminalAction::Wheel {
                lines: steps,
                at: cell,
            },
        );
        cx.notify();
    }

    pub(super) fn terminal_font_zoom(&self) -> f32 {
        self.runtime
            .settings
            .terminal_font
            .size_or(TERMINAL_FONT_SIZE)
            .as_f32()
            / TERMINAL_FONT_SIZE.as_f32()
    }

    pub(super) fn cell_width(&mut self, window: &mut Window) -> f32 {
        *self.cell_width_px.get_or_insert_with(|| {
            let mono = font(mono_family(&self.runtime.settings.terminal_font));
            let font_id = window.text_system().resolve_font(&mono);
            window
                .text_system()
                .advance(font_id, px(TERMINAL_FONT_PX), GRID_PROBE_CHAR)
                .map_or(TERMINAL_FONT_PX, |size| size.width.as_f32())
        })
    }
}
