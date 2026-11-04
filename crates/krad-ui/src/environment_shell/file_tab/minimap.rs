use gpui_kit::component::input::EditorState;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    canvas, fill, font, point, px, rgb, size, AnyElement, App, AppContext as _, Bounds,
    ContentMask, Context, DispatchPhase, Entity, FontWeight, Hitbox, HitboxBehavior, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, SharedString, Styled, Task,
    TextAlign, TextRun, Window,
};
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use super::super::context_menu::{MenuAction, MenuEntry};
use super::super::EnvironmentShell;
use crate::actions::ToggleMinimap;
use crate::components::IconName;
use crate::highlight::{highlight_lines, Syntax, TokenKind};
use crate::theme::{palette, tinted, Palette};
use view::{minimap_view, MinimapView, LINE_PX};

mod view;

const MINIMAP_WIDTH_PX: f32 = 96.0;
const HIDE_MINIMAP_LABEL: &str = "Hide minimap";
const SHOW_AGAIN_NOTE: &str = "Turn it back on in Settings \u{203a} Editor or from the palette.";
const FONT_PX: f32 = 2.0;
const LEFT_PAD_PX: f32 = 6.0;
const MAX_COLUMNS: usize = 120;
const REBUILD_DELAY: Duration = Duration::from_millis(150);
const SLIDER_ALPHA: u32 = 0x24;
const SLIDER_DRAG_ALPHA: u32 = 0x38;

#[derive(Default)]
pub(super) struct MinimapText {
    lines: Vec<MinimapLine>,
}

struct MinimapLine {
    text: SharedString,
    runs: Vec<(usize, Option<TokenKind>)>,
}

fn clipped(line: &str) -> &str {
    line.char_indices()
        .nth(MAX_COLUMNS)
        .map_or(line, |(end, _)| &line[..end])
}

fn line_runs(len: usize, spans: &[crate::highlight::Span]) -> Vec<(usize, Option<TokenKind>)> {
    let mut runs = Vec::new();
    let mut at = 0;
    for span in spans {
        let Range { start, end } = span.range.start.min(len)..span.range.end.min(len);
        if start < at || start >= end {
            continue;
        }
        if start > at {
            runs.push((start - at, None));
        }
        runs.push((end - start, Some(span.kind)));
        at = end;
    }
    if at < len {
        runs.push((len - at, None));
    }
    runs
}

pub(super) fn minimap_text(text: &str, syntax: Syntax) -> MinimapText {
    let spans = highlight_lines(text, syntax);
    let lines = text
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let shown = clipped(line);
            MinimapLine {
                runs: line_runs(shown.len(), spans.get(index).map_or(&[], Vec::as_slice)),
                text: SharedString::from(shown.to_owned()),
            }
        })
        .collect();
    MinimapText { lines }
}

#[derive(Clone, Copy)]
struct Grab {
    from_slider_top: f32,
}

pub(super) struct Minimap {
    text: Arc<MinimapText>,
    building: Option<Task<()>>,
    grab: Option<Grab>,
    hovered: bool,
}

impl Minimap {
    pub(super) fn new(text: &str, syntax: Syntax) -> Self {
        Self {
            text: Arc::new(minimap_text(text, syntax)),
            building: None,
            grab: None,
            hovered: false,
        }
    }

    pub(super) fn rebuild(&mut self, text: String, syntax: Syntax, cx: &mut Context<Self>) {
        let wait = cx.background_executor().timer(REBUILD_DELAY);
        let built = cx.background_spawn(async move {
            wait.await;
            minimap_text(&text, syntax)
        });
        self.building = Some(cx.spawn(async move |this, cx| {
            let text = built.await;
            this.update(cx, |minimap, cx| {
                minimap.text = Arc::new(text);
                minimap.building = None;
                cx.notify();
            })
            .unwrap_or_default();
        }));
    }
}

struct EditorView {
    top_line: f32,
    visible: f32,
    line_height: Pixels,
    scroll_x: Pixels,
}

fn editor_view(editor: &EditorState) -> Option<EditorView> {
    let line_height = editor.line_height()?;
    let offset = editor.scroll_offset();
    Some(EditorView {
        top_line: (-offset.y / line_height).max(0.0),
        visible: editor.input_bounds().size.height / line_height,
        line_height,
        scroll_x: offset.x,
    })
}

fn scroll_editor_to(editor: &Entity<EditorState>, top_line: f32, cx: &mut App) {
    editor.update(cx, |state, cx| {
        let Some(view) = editor_view(state) else {
            return;
        };
        let y = -(view.line_height * top_line.max(0.0));
        state.set_scroll_offset(point(view.scroll_x, y), cx);
    });
}

struct Painted {
    hitbox: Hitbox,
    view: MinimapView,
    visible: f32,
}

fn prepare(
    minimap: &Entity<Minimap>,
    editor: &Entity<EditorState>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &App,
) -> Option<Painted> {
    let shown = editor_view(editor.read(cx))?;
    let lines = minimap.read(cx).text.lines.len();
    Some(Painted {
        hitbox: window.insert_hitbox(bounds, HitboxBehavior::BlockMouse),
        view: minimap_view(
            lines,
            shown.top_line,
            shown.visible,
            f32::from(bounds.size.height),
        ),
        visible: shown.visible,
    })
}

fn line_runs_for(line: &MinimapLine, base: &gpui_kit::Font, pal: &Palette) -> Vec<TextRun> {
    line.runs
        .iter()
        .map(|(len, kind)| TextRun {
            len: *len,
            font: base.clone(),
            color: rgb(kind.map_or(pal.text_muted, |kind| pal.syntax.of(kind))).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        })
        .collect()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn paint_lines(
    text: &MinimapText,
    view: MinimapView,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let pal = palette(cx);
    let mut base = font(cx.theme().mono_font_family.clone());
    base.weight = FontWeight::BLACK;
    let first = (view.offset / LINE_PX).floor() as usize;
    let rows = (f32::from(bounds.size.height) / LINE_PX).ceil() as usize + 1;
    for (row, line) in text.lines.iter().enumerate().skip(first).take(rows) {
        if line.text.is_empty() {
            continue;
        }
        let runs = line_runs_for(line, &base, pal);
        let shaped = window
            .text_system()
            .shape_line(line.text.clone(), px(FONT_PX), &runs, None);
        #[allow(clippy::cast_precision_loss)]
        let top = bounds.top() + px(row as f32 * LINE_PX - view.offset);
        shaped
            .paint(
                point(bounds.left() + px(LEFT_PAD_PX), top),
                px(LINE_PX),
                TextAlign::Left,
                None,
                window,
                cx,
            )
            .unwrap_or_default();
    }
}

fn paint_slider(
    view: MinimapView,
    dragging: bool,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &App,
) {
    let pal = palette(cx);
    let alpha = if dragging {
        SLIDER_DRAG_ALPHA
    } else {
        SLIDER_ALPHA
    };
    let slider = Bounds::new(
        point(bounds.left(), bounds.top() + px(view.slider_top)),
        size(bounds.size.width, px(view.slider_height)),
    );
    window.paint_quad(fill(slider, tinted(pal.text_default, alpha)));
}

fn listen(
    minimap: &Entity<Minimap>,
    editor: &Entity<EditorState>,
    painted: &Painted,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let (hitbox, view, visible) = (painted.hitbox.clone(), painted.view, painted.visible);
    let (down_minimap, down_editor) = (minimap.clone(), editor.clone());
    let down_hitbox = hitbox.clone();
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble
            || event.button != MouseButton::Left
            || !down_hitbox.is_hovered(window)
        {
            return;
        }
        let y = f32::from(event.position.y - bounds.top());
        let on_slider = (view.slider_top..view.slider_top + view.slider_height).contains(&y);
        let from_slider_top = if on_slider {
            y - view.slider_top
        } else {
            scroll_editor_to(&down_editor, view.line_at(y) - visible / 2.0, cx);
            view.slider_height / 2.0
        };
        down_minimap.update(cx, |minimap, cx| {
            minimap.grab = Some(Grab { from_slider_top });
            cx.notify();
        });
        cx.stop_propagation();
    });
    let (move_minimap, move_editor) = (minimap.clone(), editor.clone());
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble {
            return;
        }
        let hovered = hitbox.is_hovered(window);
        let grab = move_minimap.read(cx).grab;
        if let Some(grab) = grab.filter(|_| event.pressed_button == Some(MouseButton::Left)) {
            let y = f32::from(event.position.y - bounds.top());
            scroll_editor_to(
                &move_editor,
                view.top_line_for_slider(y - grab.from_slider_top),
                cx,
            );
            cx.stop_propagation();
        }
        if move_minimap.read(cx).hovered != hovered {
            move_minimap.update(cx, |minimap, cx| {
                minimap.hovered = hovered;
                cx.notify();
            });
        }
    });
    let up_minimap = minimap.clone();
    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
        if phase != DispatchPhase::Bubble || up_minimap.read(cx).grab.is_none() {
            return;
        }
        up_minimap.update(cx, |minimap, cx| {
            minimap.grab = None;
            cx.notify();
        });
    });
}

fn paint(
    minimap: &Entity<Minimap>,
    editor: &Entity<EditorState>,
    painted: Option<Painted>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(painted) = painted else {
        return;
    };
    let shown = minimap.read(cx);
    let (text, dragging, hovered) = (shown.text.clone(), shown.grab.is_some(), shown.hovered);
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        paint_lines(&text, painted.view, bounds, window, cx);
        if hovered || dragging {
            paint_slider(painted.view, dragging, bounds, window, cx);
        }
    });
    listen(minimap, editor, &painted, bounds, window);
}

pub(super) fn minimap(minimap: Entity<Minimap>, editor: Entity<EditorState>) -> AnyElement {
    let (painted_minimap, painted_editor) = (minimap.clone(), editor.clone());
    canvas(
        move |bounds, window, cx| prepare(&minimap, &editor, bounds, window, cx),
        move |bounds, painted, window, cx| {
            paint(
                &painted_minimap,
                &painted_editor,
                painted,
                bounds,
                window,
                cx,
            );
        },
    )
    .flex_none()
    .w(px(MINIMAP_WIDTH_PX))
    .h_full()
    .into_any_element()
}

pub(super) fn new_minimap(text: &str, syntax: Syntax, cx: &mut App) -> Entity<Minimap> {
    cx.new(|_| Minimap::new(text, syntax))
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn toggle_minimap(&mut self, cx: &mut Context<Self>) {
        self.settings.editor.minimap = self.settings.editor.minimap.toggled();
        self.save_settings(cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn hide_minimap(&mut self, cx: &mut Context<Self>) {
        if !self.settings.editor.minimap.is_shown() {
            return;
        }
        self.toggle_minimap(cx);
    }

    pub(super) fn minimap_menu(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let entries = vec![
            MenuEntry::item(
                HIDE_MINIMAP_LABEL,
                Some(IconName::ListView),
                MenuAction::HideMinimap,
            ),
            MenuEntry::Note(SHOW_AGAIN_NOTE.to_owned()),
        ];
        self.open_menu_at(event, entries, cx);
    }

    pub(in crate::environment_shell) fn on_toggle_minimap(
        &mut self,
        _: &ToggleMinimap,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_minimap(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlight::Span;

    #[test]
    fn runs_cover_the_clipped_line_with_its_tokens_in_order() {
        let spans = [
            Span {
                range: 0..2,
                kind: TokenKind::Keyword,
            },
            Span {
                range: 3..200,
                kind: TokenKind::String,
            },
        ];
        assert_eq!(
            line_runs(10, &spans),
            [
                (2, Some(TokenKind::Keyword)),
                (1, None),
                (7, Some(TokenKind::String))
            ]
        );
        assert_eq!(line_runs(4, &[]), [(4, None)]);
        assert_eq!(
            clipped(&"é".repeat(MAX_COLUMNS + 5)).chars().count(),
            MAX_COLUMNS
        );
    }
}
