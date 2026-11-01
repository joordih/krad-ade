use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    canvas, div, fill, point, px, rgb, size, AnyElement, App, Bounds, Context, FontWeight,
    InteractiveElement, IntoElement, KeyDownEvent, MouseMoveEvent, ParentElement, Pixels, Rgba,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::ArcadeGame;
use std::rc::Rc;

use super::super::rename::KEY_ESCAPE;
use super::super::EnvironmentShell;
use super::board::{Dot, Heading, Ink, Play, Steer, COLS, ROWS};
use super::cabinet::Cabinet;
use crate::theme::{palette, tinted, Palette, LINE_XS, TEXT_XS};

const BANNER_ID: &str = "arcade-banner";
const NEXT_GAME_ID: &str = "arcade-next-game";
const BANNER_H: f32 = 170.0;
const HEADER_GAP: f32 = 6.0;
const HEADER_SPACING: f32 = 10.0;
const DOT_FILL: f32 = 0.72;
const GRID_ALPHA: u32 = 0x12;
const FAINT_ALPHA: u32 = 0x26;
const LIT_ALPHA: u32 = 0x4d;
const PLAYER_ALPHA: u32 = 0x8c;
const KEY_SPACE: &str = "space";
const KEY_UP: &str = "up";
const KEY_DOWN: &str = "down";
const KEY_LEFT: &str = "left";
const KEY_RIGHT: &str = "right";
const KEY_W: &str = "w";
const KEY_A: &str = "a";
const KEY_S: &str = "s";
const KEY_D: &str = "d";
const CRASHED_LABEL: &str = "[ game over · click to play again ]";
const NEXT_GAME_LABEL: &str = "next game ›";
const SCORE_SEPARATOR: &str = " · best ";
const SCORE_DIGITS: usize = 5;

#[derive(Clone, Copy)]
struct Inks {
    grid: Rgba,
    faint: Rgba,
    lit: Rgba,
    player: Rgba,
}

#[derive(Clone, Copy)]
struct Grid {
    left: f32,
    top: f32,
    pitch: f32,
}

impl Grid {
    fn of(bounds: Bounds<Pixels>) -> Self {
        let (width, height) = (bounds.size.width.as_f32(), bounds.size.height.as_f32());
        let pitch = (width / COLS).min(height / ROWS);
        Self {
            left: bounds.origin.x.as_f32() + (width - pitch * COLS) / 2.0,
            top: bounds.origin.y.as_f32() + (height - pitch * ROWS) / 2.0,
            pitch,
        }
    }

    fn square(self, column: f32, row: f32) -> Bounds<Pixels> {
        let side = self.pitch * DOT_FILL;
        Bounds::new(
            point(
                px(self.left + column * self.pitch),
                px(self.top + row * self.pitch),
            ),
            size(px(side), px(side)),
        )
    }

    fn column_at(self, x: f32) -> f32 {
        (x - self.left) / self.pitch
    }
}

const fn demo_caption(game: ArcadeGame) -> &'static str {
    match game {
        ArcadeGame::ByteRun => "[ take control · byte run ]",
        ArcadeGame::BugHunt => "[ take control · bug hunt ]",
        ArcadeGame::LineBreak => "[ take control · line break ]",
        ArcadeGame::TokenTrail => "[ take control · token trail ]",
    }
}

const fn keys_label(game: ArcadeGame) -> &'static str {
    match game {
        ArcadeGame::ByteRun => "space to jump · esc to leave",
        ArcadeGame::BugHunt => "arrows to steer · esc to leave",
        ArcadeGame::LineBreak => "arrows or mouse to move · esc to leave",
        ArcadeGame::TokenTrail => "arrows to turn · esc to leave",
    }
}

fn steer_of(key: &str) -> Option<Steer> {
    match key {
        KEY_SPACE => Some(Steer::Action),
        KEY_UP | KEY_W => Some(Steer::Turn(Heading::Up)),
        KEY_DOWN | KEY_S => Some(Steer::Turn(Heading::Down)),
        KEY_LEFT | KEY_A => Some(Steer::Turn(Heading::Left)),
        KEY_RIGHT | KEY_D => Some(Steer::Turn(Heading::Right)),
        _ => None,
    }
}

fn paint_matrix(bounds: Bounds<Pixels>, dots: &[Dot], inks: Inks, window: &mut Window) {
    let grid = Grid::of(bounds);
    for row in (0u8..).map(f32::from).take_while(|row| *row < ROWS) {
        for column in (0u8..).map(f32::from).take_while(|column| *column < COLS) {
            window.paint_quad(fill(grid.square(column, row), inks.grid));
        }
    }
    for dot in dots {
        let color = match dot.ink {
            Ink::Faint => inks.faint,
            Ink::Lit => inks.lit,
            Ink::Player => inks.player,
        };
        window.paint_quad(fill(grid.square(dot.column, dot.row), color));
    }
}

fn score_line(cabinet: &Cabinet) -> String {
    format!(
        "{:0width$}{SCORE_SEPARATOR}{:0width$}",
        cabinet.points(),
        cabinet.best(),
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
    fn arcade_header(&self, cx: &mut Context<Self>) -> gpui_kit::Div {
        let cabinet = &self.arcade.cabinet;
        let pal = palette(cx);
        let play = cabinet.play();
        let keys = (play == Play::Playing).then(|| label(keys_label(cabinet.game), pal));
        let score = (play != Play::Demo).then(|| {
            div()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_muted))
                .child(score_line(cabinet))
        });
        let next = div()
            .id(NEXT_GAME_ID)
            .cursor_pointer()
            .hover(|style| style.text_color(rgb(pal.text_strong)))
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                cx.stop_propagation();
                this.arcade.cabinet.next_game();
                cx.notify();
            }))
            .child(label(NEXT_GAME_LABEL, pal));
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(HEADER_SPACING))
            .h(px(LINE_XS))
            .mb(px(HEADER_GAP))
            .children(keys)
            .child(div().flex_1())
            .children(score)
            .child(next)
    }

    pub(in crate::environment_shell) fn arcade_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let arcade = &self.arcade;
        arcade.drawn.set(true);
        let cabinet = &arcade.cabinet;
        let pal = palette(cx);
        let dots = cabinet.dots();
        let ink = pal.text_muted;
        let inks = Inks {
            grid: tinted(ink, GRID_ALPHA),
            faint: tinted(ink, FAINT_ALPHA),
            lit: tinted(ink, LIT_ALPHA),
            player: tinted(pal.text_subtle, PLAYER_ALPHA),
        };
        let caption = match cabinet.play() {
            Play::Demo => Some(demo_caption(cabinet.game)),
            Play::Playing => None,
            Play::Crashed => Some(CRASHED_LABEL),
        };
        let painted = Rc::clone(&arcade.painted);
        let matrix = div()
            .relative()
            .w_full()
            .h(px(BANNER_H))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, _: &mut App| {
                        painted.set(Some(bounds));
                        paint_matrix(bounds, &dots, inks, window);
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
                this.arcade.cabinet.take(Steer::Action);
                cx.notify();
            }))
            .on_mouse_move(
                cx.listener(|this: &mut Self, event: &MouseMoveEvent, _, _| {
                    let Some(bounds) = this.arcade.painted.get() else {
                        return;
                    };
                    let column = Grid::of(bounds).column_at(event.position.x.as_f32());
                    this.arcade.cabinet.aim(column);
                }),
            )
            .on_key_down(
                cx.listener(|this: &mut Self, event: &KeyDownEvent, window, cx| {
                    let key = event.keystroke.key.as_str();
                    if key == KEY_ESCAPE {
                        this.arcade.cabinet.leave();
                        window.focus(&this.focus, cx);
                    } else if let Some(steer) = steer_of(key) {
                        this.arcade.cabinet.take(steer);
                    } else {
                        return;
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(self.arcade_header(cx))
            .child(matrix)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_and_wasd_steer_the_same_way_and_space_acts() {
        for (arrow, letter) in [
            (KEY_UP, KEY_W),
            (KEY_DOWN, KEY_S),
            (KEY_LEFT, KEY_A),
            (KEY_RIGHT, KEY_D),
        ] {
            assert!(steer_of(arrow).is_some());
            assert_eq!(steer_of(arrow), steer_of(letter));
        }
        assert_eq!(steer_of(KEY_SPACE), Some(Steer::Action));
        assert_eq!(steer_of("x"), None);
    }

    #[test]
    fn a_pointer_column_maps_back_onto_the_dot_grid() {
        let bounds = Bounds::new(point(px(10.0), px(0.0)), size(px(960.0), px(240.0)));
        let grid = Grid::of(bounds);
        assert!((grid.column_at(10.0)).abs() < f32::EPSILON);
        assert!((grid.column_at(10.0 + 480.0) - COLS / 2.0).abs() < 1e-3);
    }
}
