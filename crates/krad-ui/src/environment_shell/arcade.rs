mod runner;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    canvas, div, fill, point, px, rgb, size, AnyElement, App, Bounds, Context, FocusHandle,
    FontWeight, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Pixels, Rgba,
    StatefulInteractiveElement, Styled, Window,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use self::runner::{Play, Runner, COLS, GROUND_ROW, ROWS, RUNNER_COL, RUNNER_H, RUNNER_W};
use super::rename::KEY_ESCAPE;
use super::EnvironmentShell;
use crate::theme::{palette, tinted, Palette, LINE_XS, TEXT_XS};

const BANNER_ID: &str = "arcade-banner";
const BANNER_H: f32 = 104.0;
const HEADER_GAP: f32 = 6.0;
const DOT_FILL: f32 = 0.72;
const GRID_ALPHA: u32 = 0x12;
const LIT_ALPHA: u32 = 0x4d;
const PLAYER_ALPHA: u32 = 0x8c;
const KEY_SPACE: &str = "space";
const KEY_UP: &str = "up";
const DEMO_LABEL: &str = "[ take control · byte run ]";
const CRASHED_LABEL: &str = "[ crashed · click to run again ]";
const PLAYING_LABEL: &str = "space to jump · esc to leave";
const SCORE_SEPARATOR: &str = " · best ";
const SCORE_DIGITS: usize = 5;

pub(super) struct Arcade {
    runner: Runner,
    focus: FocusHandle,
    last_frame: Option<Instant>,
    drawn: Cell<bool>,
    painted: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Arcade {
    pub(super) fn new(focus: FocusHandle) -> Self {
        Self {
            runner: Runner::new(),
            focus,
            last_frame: None,
            drawn: Cell::new(false),
            painted: Rc::new(Cell::new(None)),
        }
    }
}

#[derive(Clone, Copy)]
enum Ink {
    Lit,
    Player,
}

fn lit_dots(runner: &Runner) -> Vec<(f32, f32, Ink)> {
    let ground = (0u8..)
        .map(f32::from)
        .take_while(|column| *column < COLS)
        .map(|column| (column, GROUND_ROW, Ink::Lit));
    let feet = GROUND_ROW - 1.0 - runner.height.round();
    let caret = [0.0, 1.0].into_iter().flat_map(move |dx| {
        [0.0, 1.0, 2.0]
            .into_iter()
            .take_while(|dy| *dy < RUNNER_H)
            .map(move |dy| (RUNNER_COL + dx, feet - dy, Ink::Player))
    });
    let bugs = runner.obstacles.iter().flat_map(|obstacle| {
        let left = obstacle.x.floor();
        obstacle
            .bug
            .dots()
            .map(move |(column, height)| (left + column, GROUND_ROW - 1.0 - height, Ink::Lit))
    });
    ground
        .chain(caret.take_while(|(column, ..)| *column < RUNNER_COL + RUNNER_W))
        .chain(bugs.filter(|(column, ..)| (0.0..COLS).contains(column)))
        .collect()
}

fn paint_matrix(
    bounds: Bounds<gpui_kit::Pixels>,
    dots: &[(f32, f32, Ink)],
    colors: (Rgba, Rgba, Rgba),
    window: &mut Window,
) {
    let (grid, lit, player) = colors;
    let pitch = (bounds.size.width.as_f32() / COLS).min(bounds.size.height.as_f32() / ROWS);
    let side = pitch * DOT_FILL;
    let left = bounds.origin.x.as_f32() + (bounds.size.width.as_f32() - pitch * COLS) / 2.0;
    let top = bounds.origin.y.as_f32() + (bounds.size.height.as_f32() - pitch * ROWS) / 2.0;
    let square = |column: f32, row: f32| {
        Bounds::new(
            point(px(left + column * pitch), px(top + row * pitch)),
            size(px(side), px(side)),
        )
    };
    for row in (0u8..).map(f32::from).take_while(|row| *row < ROWS) {
        for column in (0u8..).map(f32::from).take_while(|column| *column < COLS) {
            window.paint_quad(fill(square(column, row), grid));
        }
    }
    for &(column, row, ink) in dots {
        let color = match ink {
            Ink::Lit => lit,
            Ink::Player => player,
        };
        window.paint_quad(fill(square(column, row), color));
    }
}

fn score_line(runner: &Runner) -> String {
    format!(
        "{:0width$}{SCORE_SEPARATOR}{:0width$}",
        runner.points,
        runner.best,
        width = SCORE_DIGITS
    )
}

fn label(text: &'static str, pal: &Palette) -> gpui_kit::Div {
    div()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(pal.text_subtle))
        .child(text)
}

impl EnvironmentShell {
    pub(super) fn tick_arcade(&mut self, now: Instant) -> bool {
        let arcade = &mut self.arcade;
        if !arcade.drawn.replace(false) {
            arcade.last_frame = None;
            arcade.painted.set(None);
            return false;
        }
        if let Some(last) = arcade.last_frame {
            arcade.runner.step(now.saturating_duration_since(last));
        }
        arcade.last_frame = Some(now);
        true
    }

    pub(super) fn arcade_area(&self) -> Option<Bounds<Pixels>> {
        self.arcade.painted.get()
    }

    pub(super) fn arcade_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let arcade = &self.arcade;
        arcade.drawn.set(true);
        let runner = &arcade.runner;
        let pal = palette(cx);
        let dots = lit_dots(runner);
        let ink = pal.text_muted;
        let colors = (
            tinted(ink, GRID_ALPHA),
            tinted(ink, LIT_ALPHA),
            tinted(pal.text_subtle, PLAYER_ALPHA),
        );
        let caption = match runner.play {
            Play::Demo => Some(DEMO_LABEL),
            Play::Playing => None,
            Play::Crashed => Some(CRASHED_LABEL),
        };
        let keys = (runner.play == Play::Playing).then(|| label(PLAYING_LABEL, pal));
        let score = (runner.play != Play::Demo).then(|| score_line(runner));
        let painted = Rc::clone(&arcade.painted);
        let header = div()
            .flex()
            .flex_row()
            .justify_between()
            .h(px(LINE_XS))
            .mb(px(HEADER_GAP))
            .children(keys)
            .child(div().flex_1())
            .children(score.map(|score| {
                div()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(score)
            }));
        let matrix = div()
            .relative()
            .w_full()
            .h(px(BANNER_H))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, _: &mut App| {
                        painted.set(Some(bounds));
                        paint_matrix(bounds, &dots, colors, window);
                    },
                )
                .size_full(),
            )
            .when_some(caption, |matrix, caption| {
                matrix.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(label(caption, pal)),
                )
            });
        div()
            .id(BANNER_ID)
            .flex()
            .flex_col()
            .w_full()
            .cursor_pointer()
            .track_focus(&arcade.focus)
            .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                window.focus(&this.arcade.focus, cx);
                this.arcade.runner.press();
                cx.notify();
            }))
            .on_key_down(
                cx.listener(|this: &mut Self, event: &KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        KEY_SPACE | KEY_UP => this.arcade.runner.press(),
                        KEY_ESCAPE => {
                            this.arcade.runner.release();
                            window.focus(&this.focus, cx);
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(header)
            .child(matrix)
            .into_any_element()
    }
}
