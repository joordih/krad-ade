use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, font, px, rgb, AnyElement, App, Div, Font, IntoElement, ParentElement, SharedString,
    Styled,
};
use krad_domain::Ligatures;
use krad_runtime::TerminalFrame;
use krad_session::{CursorShape, ImageDepth, TermCursor};
use std::sync::Arc;

use super::super::live_terminal::LiveTerminal;
use super::super::terminal_images::{image_layer, ImageGrid, TerminalImages};
use super::super::terminal_text::{terminal_text, TextLook};
use super::geometry::{BodyGeometry, BODY_PAD_BOTTOM, BODY_PAD_TOP, BODY_PAD_X, TERMINAL_FONT_PX};
use crate::terminal_theme::TerminalColors;
use crate::theme::{font_features, palette, terminal_fill, tinted};

const CURSOR_FILL_ALPHA: u32 = 0x8c;
const CURSOR_BAR_PX: f32 = 2.0;
const CURSOR_OUTLINE_PX: f32 = 1.0;

pub(super) fn session_screen(
    body: Div,
    session: &LiveTerminal,
    uploaded: Option<&TerminalImages>,
    look: BodyLook,
    mono: &SharedString,
    colors: &TerminalColors,
) -> AnyElement {
    frame_screen(
        body,
        session.view.paint_frame(),
        uploaded,
        look,
        (mono, colors),
    )
}

pub(in crate::environment_shell) fn frame_screen(
    body: Div,
    frame: Arc<TerminalFrame>,
    uploaded: Option<&TerminalImages>,
    look: BodyLook,
    (mono, colors): (&SharedString, &TerminalColors),
) -> AnyElement {
    let BodyLook {
        geometry,
        zoom,
        focused,
        ..
    } = look;
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
    let layer = |depth| {
        frame.cell.map_or_else(Vec::new, |placed_with| {
            let grid = ImageGrid {
                left: BODY_PAD_X * zoom,
                top: BODY_PAD_TOP * zoom,
                cell_w: geometry.cell_w,
                line_h: geometry.line_h,
                placed_with,
            };
            image_layer(&frame.screen.images, uploaded, depth, grid)
        })
    };
    let below = layer(ImageDepth::BelowText);
    let above = layer(ImageDepth::AboveText);
    let cursor = frame.screen.cursor;
    body.children(below)
        .child(terminal_text(frame, text_look))
        .children(cursor.map(|cursor| cursor_element(cursor, geometry, zoom, focused, colors)))
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

fn count_px(count: usize) -> f32 {
    u16::try_from(count).map_or(f32::from(u16::MAX), f32::from)
}

#[derive(Clone, Copy)]
pub(in crate::environment_shell) struct BodyLook {
    pub(in crate::environment_shell) geometry: BodyGeometry,
    pub(in crate::environment_shell) ligatures: Ligatures,
    pub(in crate::environment_shell) zoom: f32,
    pub(in crate::environment_shell) text_zoom: f32,
    pub(in crate::environment_shell) focused: bool,
    pub(in crate::environment_shell) corner: f32,
}

pub(in crate::environment_shell) fn embedded_screen(
    session: &LiveTerminal,
    look: BodyLook,
    mono: &SharedString,
    colors: &TerminalColors,
    cx: &App,
) -> AnyElement {
    let body = screen_body(look, mono, colors, cx);
    session_screen(body, session, None, look, mono, colors)
}

pub(in crate::environment_shell) fn screen_body(
    look: BodyLook,
    mono: &SharedString,
    colors: &TerminalColors,
    cx: &App,
) -> Div {
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .px(px(BODY_PAD_X * look.zoom))
        .pt(px(BODY_PAD_TOP * look.zoom))
        .pb(px(BODY_PAD_BOTTOM * look.zoom))
        .rounded(px(look.corner))
        .when_some(
            terminal_fill(colors.background, palette(cx), cx),
            Styled::bg,
        )
        .font_family(mono.clone())
        .font_features(font_features(look.ligatures))
        .text_size(px(TERMINAL_FONT_PX * look.text_zoom))
        .line_height(px(look.geometry.line_h))
        .text_color(rgb(colors.foreground))
        .whitespace_nowrap()
}
