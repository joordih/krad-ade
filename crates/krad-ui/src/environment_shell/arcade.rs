mod board;
mod bricks;
mod cabinet;
mod maze;
mod runner;
mod trail;
mod view;

use gpui_kit::{AppContext as _, Bounds, Context, FocusHandle, Pixels};
use krad_store::{read_arcade_scores, write_arcade_scores};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use self::cabinet::Cabinet;
use super::EnvironmentShell;

const SCORES_NOT_SAVED: &str = "Couldn't save the arcade's best scores";

pub(super) struct Arcade {
    cabinet: Cabinet,
    focus: FocusHandle,
    last_frame: Option<Instant>,
    drawn: Cell<bool>,
    painted: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Arcade {
    pub(super) fn new(focus: FocusHandle) -> Self {
        Self {
            cabinet: Cabinet::new(),
            focus,
            last_frame: None,
            drawn: Cell::new(false),
            painted: Rc::new(Cell::new(None)),
        }
    }
}

impl EnvironmentShell {
    pub(super) fn load_arcade_scores(cx: &mut Context<Self>) {
        let reading = cx.background_spawn(async move { read_arcade_scores() });
        cx.spawn(async move |this, cx| {
            let scores = reading.await;
            this.update(cx, |shell, _| shell.arcade.cabinet.scores = scores)
                .unwrap_or_default();
        })
        .detach();
    }

    fn save_arcade_scores(&self, cx: &mut Context<Self>) {
        let scores = self.arcade.cabinet.scores.clone();
        let writing = cx.background_spawn(async move { write_arcade_scores(&scores) });
        cx.spawn(async move |this, cx| {
            let Err(error) = writing.await else {
                return;
            };
            this.update(cx, |shell, cx| {
                shell.push_toast(format!("{SCORES_NOT_SAVED}: {error}"));
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn draw_arcade_game(&mut self) {
        self.arcade.cabinet.draw();
    }

    pub(super) fn tick_arcade(&mut self, now: Instant, cx: &mut Context<Self>) -> bool {
        let arcade = &mut self.arcade;
        if !arcade.drawn.replace(false) {
            arcade.last_frame = None;
            arcade.painted.set(None);
            return false;
        }
        let new_best = arcade
            .last_frame
            .is_some_and(|last| arcade.cabinet.step(now.saturating_duration_since(last)));
        arcade.last_frame = Some(now);
        if new_best {
            self.save_arcade_scores(cx);
        }
        true
    }

    pub(super) fn arcade_area(&self) -> Option<Bounds<Pixels>> {
        self.arcade.painted.get()
    }
}
