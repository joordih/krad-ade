use gpui_kit::component::input::{EditorState, Position, RopeExt as _};
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::{
    canvas, fill, point, px, rgb, size, AnyElement, App, Bounds, CursorStyle, DispatchPhase,
    Entity, Hitbox, HitboxBehavior, IntoElement, MouseButton, MouseDownEvent, Pixels, Styled,
    Window,
};
use krad_domain::ProblemSeverity;

use super::problem_list::ProblemList;
use crate::theme::palette;

const MARK_HEIGHT_PX: f32 = 3.0;
const MARK_INSET_PX: f32 = 3.0;

struct Mark {
    line: u32,
    hitbox: Hitbox,
}

#[allow(clippy::cast_precision_loss)]
pub(super) fn mark_top(line: usize, lines: usize, track: Bounds<Pixels>) -> Pixels {
    let room = (track.size.height - px(MARK_HEIGHT_PX)).max(px(0.0));
    let fraction = line as f32 / lines.max(1) as f32;
    track.top() + room * fraction.clamp(0.0, 1.0)
}

fn error_lines(problems: &ProblemList) -> Vec<usize> {
    let mut lines: Vec<usize> = problems
        .problems
        .iter()
        .filter(|problem| problem.severity == ProblemSeverity::Error)
        .map(|problem| problem.line)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

fn place_marks(
    state: &Entity<EditorState>,
    problems: &Entity<ProblemList>,
    track: Bounds<Pixels>,
    window: &mut Window,
    cx: &App,
) -> Vec<Mark> {
    let editor = state.read(cx);
    let lines = editor.text().lines_len();
    error_lines(problems.read(cx))
        .into_iter()
        .filter_map(|line| {
            let bounds = Bounds::new(
                point(
                    track.left() + px(MARK_INSET_PX),
                    mark_top(line, lines, track),
                ),
                size(
                    (track.size.width - px(MARK_INSET_PX * 2.0)).max(px(1.0)),
                    px(MARK_HEIGHT_PX),
                ),
            );
            Some(Mark {
                line: u32::try_from(line).ok()?,
                hitbox: window.insert_hitbox(bounds, HitboxBehavior::BlockMouse),
            })
        })
        .collect()
}

fn paint_marks(state: &Entity<EditorState>, marks: Vec<Mark>, window: &mut Window, cx: &App) {
    let color = rgb(palette(cx).error_fg);
    for mark in &marks {
        window.paint_quad(fill(mark.hitbox.bounds, color));
        window.set_cursor_style(CursorStyle::PointingHand, &mark.hitbox);
    }
    let state = state.clone();
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
            return;
        }
        let Some(mark) = marks.iter().find(|mark| mark.hitbox.is_hovered(window)) else {
            return;
        };
        let line = mark.line;
        state.update(cx, |state, cx| {
            state.set_cursor_position(Position::new(line, 0), window, cx);
            state.focus(window, cx);
        });
        cx.stop_propagation();
    });
}

pub(super) fn problem_marks(
    state: Entity<EditorState>,
    problems: Entity<ProblemList>,
) -> AnyElement {
    let painted = state.clone();
    canvas(
        move |track, window, cx| place_marks(&state, &problems, track, window, cx),
        move |_, marks, window, cx| paint_marks(&painted, marks, window, cx),
    )
    .absolute()
    .top_0()
    .bottom_0()
    .right_0()
    .w(Scrollbar::width())
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_sit_at_their_line_share_of_the_track_and_stay_inside_it() {
        let track = Bounds::new(point(px(0.0), px(100.0)), size(px(16.0), px(203.0)));
        assert_eq!(mark_top(0, 100, track), px(100.0));
        assert_eq!(mark_top(50, 100, track), px(200.0));
        assert_eq!(mark_top(100, 100, track), px(300.0));
        assert_eq!(mark_top(500, 100, track), px(300.0));
        assert_eq!(mark_top(3, 0, track), px(300.0));
    }
}
