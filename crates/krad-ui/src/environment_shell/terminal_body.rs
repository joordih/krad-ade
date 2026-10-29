use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, font, px, rgb, AnyElement, Context, Div, Font, FontFeatures, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, ParentElement, ScrollWheelEvent, SharedString,
    Styled, Window,
};
use krad_domain::{Ligatures, TerminalId};
use krad_session::{
    CellPoint, CursorShape, GridSize, ImageDepth, LiveSession, MouseAction, PtyInput, TermCursor,
    WheelInput,
};
use std::sync::Arc;
use std::time::Duration;

use super::drag::DragState;
use super::terminal_images::{image_layer, ImageGrid, TerminalImages};
use super::terminal_text::{terminal_text, TextLook};
use super::EnvironmentShell;
use crate::components::thinking_indicator;
use crate::terminal_theme::TerminalColors;
use crate::theme::{palette, terminal_fill, terminal_font_family, tinted};

pub(super) const TERMINAL_FONT_PX: f32 = crate::theme::TEXT_XS;
pub(super) const TERMINAL_LINE_PX: f32 = 18.0;
pub(super) const BODY_PAD_X: f32 = 12.0;
pub(super) const BODY_PAD_TOP: f32 = crate::theme::SPACE_1;
const BODY_PAD_BOTTOM: f32 = crate::theme::SPACE_3;
const GRID_PROBE_CHAR: char = 'm';
const LIGATURE_FEATURES: [&str; 2] = ["calt", "liga"];
const FEATURE_ON: u32 = 1;
const FEATURE_OFF: u32 = 0;
const CARD_STARTING_TEXT: &str = "starting…";
const AGENT_STARTING_PREFIX: &str = "Starting";
const AGENT_STARTING_SUFFIX: &str = "…";
const AGENT_FALLBACK_NAME: &str = "the agent";
const AGENT_START_LIMIT: Duration = Duration::from_secs(20);

fn agent_starting(session: &LiveSession) -> bool {
    session.is_agent
        && !session.hooked
        && session.state.is_running()
        && session.spawned_at.elapsed() < AGENT_START_LIMIT
}

fn starting_view(
    id: TerminalId,
    session: &LiveSession,
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
            session.spawned_at,
            mono.clone(),
            palette(cx),
        ))
        .into_any_element()
}
const STARTING_OPACITY: f32 = 0.6;
const CURSOR_FILL_ALPHA: u32 = 0x8c;
const CURSOR_BAR_PX: f32 = 2.0;
const CURSOR_OUTLINE_PX: f32 = 1.0;

#[derive(Clone, Copy)]
pub(super) struct BodyGeometry {
    pub(super) origin: (f32, f32),
    pub(super) cell_w: f32,
    pub(super) line_h: f32,
}

impl BodyGeometry {
    pub(super) fn cell_at(&self, x: f32, y: f32) -> CellPoint {
        CellPoint {
            row: cell_index(y - self.origin.1, self.line_h),
            col: cell_index(x - self.origin.0, self.cell_w),
        }
    }
}

fn cell_index(offset: f32, size: f32) -> usize {
    let index = (offset / size.max(f32::EPSILON)).floor().max(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cells = index as usize;
    cells
}

pub(super) fn body_grid(
    width: f32,
    height: f32,
    header: f32,
    cell_width: f32,
    text_scale: f32,
) -> GridSize {
    let body_w = width - 2.0 * BODY_PAD_X;
    let body_h = height - header - BODY_PAD_TOP - BODY_PAD_BOTTOM;
    let (cell_w, cell_h) = (cell_width * text_scale, TERMINAL_LINE_PX * text_scale);
    GridSize {
        cols: cells_in(body_w, cell_w),
        rows: cells_in(body_h, cell_h),
        cell_width_px: whole_px(cell_w),
        cell_height_px: whole_px(cell_h),
    }
}

fn whole_px(length: f32) -> u16 {
    let rounded = length.round().clamp(0.0, f32::from(u16::MAX));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pixels = rounded as u16;
    pixels
}

fn cells_in(length: f32, cell: f32) -> u16 {
    let count = (length / cell).floor().clamp(1.0, f32::from(u16::MAX));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cells = count as u16;
    cells
}

fn session_screen(
    body: Div,
    session: &LiveSession,
    uploaded: Option<&TerminalImages>,
    look: BodyLook,
    mono: &SharedString,
    colors: &TerminalColors,
) -> AnyElement {
    let BodyLook {
        geometry,
        zoom,
        focused,
        ..
    } = look;
    let screen = session.engine.screen(&colors.ansi);
    let text_look = TextLook {
        font: Font {
            features: font_features(look.ligatures),
            ..font(mono.clone())
        },
        font_size: TERMINAL_FONT_PX * look.text_zoom,
        cell_w: geometry.cell_w,
        line_h: geometry.line_h,
        foreground: colors.foreground,
        background: colors.background,
        selection: tinted(colors.selection.0, colors.selection.1),
    };
    let grid = ImageGrid {
        left: BODY_PAD_X * zoom,
        top: BODY_PAD_TOP * zoom,
        cell_w: geometry.cell_w,
        line_h: geometry.line_h,
        placed_with: session.engine.cell_pixels(),
    };
    let layer = |depth| image_layer(&screen.images, uploaded, depth, grid);
    let below = layer(ImageDepth::BelowText);
    let above = layer(ImageDepth::AboveText);
    body.children(below)
        .child(terminal_text(screen.rows, text_look))
        .children(
            screen
                .cursor
                .map(|cursor| cursor_element(cursor, geometry, zoom, focused, colors)),
        )
        .children(above)
        .into_any_element()
}

fn cursor_element(
    cursor: TermCursor,
    geometry: BodyGeometry,
    zoom: f32,
    focused: bool,
    colors: &TerminalColors,
) -> Div {
    let left = BODY_PAD_X * zoom + count_px(cursor.col) * geometry.cell_w;
    let top = BODY_PAD_TOP * zoom + count_px(cursor.row) * geometry.line_h;
    let (width, height, offset_y) = match cursor.shape {
        CursorShape::Block => (geometry.cell_w, geometry.line_h, 0.0),
        CursorShape::Beam => (CURSOR_BAR_PX, geometry.line_h, 0.0),
        CursorShape::Underline => (
            geometry.cell_w,
            CURSOR_BAR_PX,
            geometry.line_h - CURSOR_BAR_PX,
        ),
    };
    let base = div()
        .absolute()
        .left(px(left))
        .top(px(top + offset_y))
        .w(px(width))
        .h(px(height));
    if focused || cursor.shape != CursorShape::Block {
        base.bg(tinted(colors.cursor, CURSOR_FILL_ALPHA))
    } else {
        base.border(px(CURSOR_OUTLINE_PX))
            .border_color(rgb(colors.cursor))
    }
}

fn send_input(session: &LiveSession, bytes: Vec<u8>) {
    drop(session.writer_tx.send(PtyInput::Bytes(bytes)));
}

fn count_px(count: usize) -> f32 {
    u16::try_from(count).map_or(f32::from(u16::MAX), f32::from)
}

fn font_features(ligatures: Ligatures) -> FontFeatures {
    let value = match ligatures {
        Ligatures::On => FEATURE_ON,
        Ligatures::Off => FEATURE_OFF,
    };
    FontFeatures(Arc::new(
        LIGATURE_FEATURES
            .iter()
            .map(|feature| ((*feature).to_owned(), value))
            .collect(),
    ))
}

#[derive(Clone, Copy)]
pub(super) struct BodyLook {
    pub(super) geometry: BodyGeometry,
    pub(super) ligatures: Ligatures,
    pub(super) zoom: f32,
    pub(super) text_zoom: f32,
    pub(super) focused: bool,
    pub(super) corner: f32,
}

impl EnvironmentShell {
    pub(super) fn terminal_body(
        id: TerminalId,
        session: Option<&LiveSession>,
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
        let Some(session) = live.sessions.get_mut(&id) else {
            cx.notify();
            return;
        };
        if event.modifiers.secondary() {
            if let Some(url) = session.engine.hyperlink_at(cell) {
                cx.open_url(&url);
                return;
            }
        }
        let report = (!event.modifiers.shift)
            .then(|| session.engine.click_report(MouseAction::Press, cell))
            .flatten();
        if let Some(bytes) = report {
            session.engine.clear_selection();
            send_input(session, bytes);
            self.drag = DragState::Report { id, geometry, cell };
        } else {
            session.engine.start_selection(cell);
            self.drag = DragState::Select { id, geometry };
        }
        cx.notify();
    }

    pub(super) fn report_click(&mut self, id: TerminalId, action: MouseAction, cell: CellPoint) {
        let env = self.active.clone();
        let Some(session) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.sessions.get_mut(&id))
        else {
            return;
        };
        if let Some(bytes) = session.engine.click_report(action, cell) {
            send_input(session, bytes);
        }
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
        let Some(session) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.sessions.get_mut(&id))
        else {
            return;
        };
        match session.engine.wheel(steps, cell) {
            WheelInput::History(lines) => session.engine.scroll_lines(lines),
            WheelInput::Program(bytes) => send_input(session, bytes),
        }
        cx.notify();
    }

    pub(super) fn cell_width(&mut self, window: &mut Window) -> f32 {
        *self.cell_width_px.get_or_insert_with(|| {
            let mono = font(terminal_font_family(self.settings.terminal_font));
            let font_id = window.text_system().resolve_font(&mono);
            window
                .text_system()
                .advance(font_id, px(TERMINAL_FONT_PX), GRID_PROBE_CHAR)
                .map_or(TERMINAL_FONT_PX, |size| size.width.as_f32())
        })
    }
}
