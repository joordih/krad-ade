use gpui_kit::{
    canvas, fill, point, px, rgb, size, AnyElement, App, Bounds, CursorStyle, Font, FontStyle,
    FontWeight, Hitbox, HitboxBehavior, Hsla, IntoElement, Pixels, Rgba, ShapedLine, Styled,
    TextAlign, TextRun, UnderlineStyle, Window,
};
use krad_session::{block, CellRect, TermRow, TermSpan, CELL_STEPS};

const UNDERLINE_PX: f32 = 1.0;

#[derive(Clone)]
pub(super) struct TextLook {
    pub(super) font: Font,
    pub(super) font_size: f32,
    pub(super) cell_w: f32,
    pub(super) line_h: f32,
    pub(super) foreground: u32,
    pub(super) background: u32,
    pub(super) selection: Rgba,
}

struct Prepared {
    backgrounds: Vec<(Bounds<Pixels>, Hsla)>,
    blocks: Vec<(Bounds<Pixels>, Hsla)>,
    lines: Vec<(gpui_kit::Point<Pixels>, ShapedLine)>,
    links: Vec<Hitbox>,
    line_h: Pixels,
}

fn count_px(count: usize) -> f32 {
    u16::try_from(count).map_or(f32::from(u16::MAX), f32::from)
}

fn span_colors(span: &TermSpan, look: &TextLook) -> (Hsla, Option<Hsla>) {
    let (mut fg, mut bg) = (span.fg, span.bg);
    if span.style.inverse {
        (fg, bg) = (
            Some(bg.unwrap_or(look.background)),
            Some(fg.unwrap_or(look.foreground)),
        );
    }
    let text = rgb(fg.unwrap_or(look.foreground)).into();
    let background = if span.style.selected {
        Some(look.selection.into())
    } else {
        bg.filter(|color| *color != look.background)
            .map(|color| rgb(color).into())
    };
    (text, background)
}

fn span_font(span: &TermSpan, look: &TextLook) -> Font {
    let mut font = look.font.clone();
    if span.style.bold {
        font.weight = FontWeight::BOLD;
    }
    if span.style.italic {
        font.style = FontStyle::Italic;
    }
    font
}

fn snap(length: Pixels, scale: f32) -> Pixels {
    px((length.as_f32() * scale).round() / scale)
}

fn block_bounds(cell: Bounds<Pixels>, rect: CellRect, scale: f32) -> Bounds<Pixels> {
    let step = |length: Pixels, steps: u8| length * (f32::from(steps) / f32::from(CELL_STEPS));
    let left = snap(cell.origin.x + step(cell.size.width, rect.left), scale);
    let right = snap(cell.origin.x + step(cell.size.width, rect.right), scale);
    let top = snap(cell.origin.y + step(cell.size.height, rect.top), scale);
    let bottom = snap(cell.origin.y + step(cell.size.height, rect.bottom), scale);
    Bounds::new(point(left, top), size(right - left, bottom - top))
}

fn push_blocks(span: &TermSpan, area: Bounds<Pixels>, color: Hsla, scale: f32, out: &mut Prepared) {
    let cell_w = area.size.width / count_px(span.cells.max(1));
    for (index, character) in span.text.chars().enumerate() {
        let Some(shape) = block(character) else {
            continue;
        };
        let cell = Bounds::new(
            point(area.origin.x + cell_w * count_px(index), area.origin.y),
            size(cell_w, area.size.height),
        );
        let shaded = color.opacity(shape.shade.opacity());
        out.blocks.extend(
            shape
                .rects
                .iter()
                .map(|rect| (block_bounds(cell, *rect, scale), shaded)),
        );
    }
}

fn prepare(
    rows: &[TermRow],
    look: &TextLook,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) -> Prepared {
    let mut prepared = Prepared {
        backgrounds: Vec::new(),
        blocks: Vec::new(),
        lines: Vec::new(),
        links: Vec::new(),
        line_h: px(look.line_h),
    };
    let font_size = px(look.font_size);
    let scale = window.scale_factor();
    for (row_index, row) in rows.iter().enumerate() {
        let y = bounds.origin.y + px(count_px(row_index) * look.line_h);
        let mut col = 0;
        for span in &row.spans {
            let origin = point(bounds.origin.x + px(count_px(col) * look.cell_w), y);
            let area = Bounds::new(
                origin,
                size(px(count_px(span.cells) * look.cell_w), px(look.line_h)),
            );
            col += span.cells;
            let (text_color, background) = span_colors(span, look);
            if let Some(background) = background {
                prepared.backgrounds.push((area, background));
            }
            if span.link.is_some() {
                prepared
                    .links
                    .push(window.insert_hitbox(area, HitboxBehavior::Normal));
            }
            if span.text.trim().is_empty() && !span.style.underline {
                continue;
            }
            if span.text.chars().next().and_then(block).is_some() {
                push_blocks(span, area, text_color, scale, &mut prepared);
                continue;
            }
            let run = TextRun {
                len: span.text.len(),
                font: span_font(span, look),
                color: text_color,
                background_color: None,
                underline: span.style.underline.then_some(UnderlineStyle {
                    thickness: px(UNDERLINE_PX),
                    color: Some(text_color),
                    wavy: false,
                }),
                strikethrough: None,
            };
            let shaped =
                window
                    .text_system()
                    .shape_line(span.text.clone().into(), font_size, &[run], None);
            prepared.lines.push((origin, shaped));
        }
    }
    prepared
}

fn paint(prepared: Prepared, window: &mut Window, cx: &mut App) {
    for (area, color) in prepared.backgrounds.into_iter().chain(prepared.blocks) {
        window.paint_quad(fill(area, color));
    }
    for (origin, line) in prepared.lines {
        line.paint(origin, prepared.line_h, TextAlign::Left, None, window, cx)
            .unwrap_or_default();
    }
    for link in &prepared.links {
        window.set_cursor_style(CursorStyle::PointingHand, link);
    }
}

pub(super) fn terminal_text(rows: Vec<TermRow>, look: TextLook) -> AnyElement {
    let height = count_px(rows.len()) * look.line_h;
    canvas(
        move |bounds, window, _| prepare(&rows, &look, bounds, window),
        move |_, prepared, window, cx| paint(prepared, window, cx),
    )
    .w_full()
    .h(px(height))
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{font, rgba};
    use krad_session::SpanStyle;

    const BACKGROUND: u32 = 0x0028_2a36;
    const RED: u32 = 0x00ff_5555;

    fn look() -> TextLook {
        TextLook {
            font: font("SF Mono"),
            font_size: 12.0,
            cell_w: 7.0,
            line_h: 16.0,
            foreground: 0x00f8_f8f2,
            background: BACKGROUND,
            selection: rgba(0x4447_5aff),
        }
    }

    fn span(bg: Option<u32>) -> TermSpan {
        TermSpan {
            text: "x".to_owned(),
            cells: 1,
            fg: None,
            bg,
            style: SpanStyle::default(),
            link: None,
        }
    }

    #[test]
    fn a_cell_in_the_theme_background_lets_the_body_show_through() {
        assert_eq!(span_colors(&span(Some(BACKGROUND)), &look()).1, None);
        assert_eq!(
            span_colors(&span(Some(RED)), &look()).1,
            Some(rgb(RED).into())
        );
        assert_eq!(span_colors(&span(None), &look()).1, None);
    }
}
