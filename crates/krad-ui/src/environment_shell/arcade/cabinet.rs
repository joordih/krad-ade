use krad_domain::{ArcadeGame, ArcadeScores};
use std::time::Duration;

use super::board::{Board, Dice, Dot, Play, Steer};
use super::bricks::Bricks;
use super::maze::Maze;
use super::runner::Runner;
use super::trail::Trail;

fn board_of(game: ArcadeGame) -> Box<dyn Board> {
    match game {
        ArcadeGame::ByteRun => Box::new(Runner::new()),
        ArcadeGame::BugHunt => Box::new(Maze::new()),
        ArcadeGame::LineBreak => Box::new(Bricks::new()),
        ArcadeGame::TokenTrail => Box::new(Trail::new()),
    }
}

pub(super) struct Cabinet {
    pub(super) game: ArcadeGame,
    board: Box<dyn Board>,
    pub(super) scores: ArcadeScores,
    recorded: bool,
    dice: Dice,
}

impl Cabinet {
    pub(super) fn new() -> Self {
        Self::with_dice(Dice::new())
    }

    fn with_dice(mut dice: Dice) -> Self {
        let game = ArcadeGame::drawn(None, dice.roll());
        Self {
            game,
            board: board_of(game),
            scores: ArcadeScores::default(),
            recorded: false,
            dice,
        }
    }

    fn show(&mut self, game: ArcadeGame) {
        self.game = game;
        self.board = board_of(game);
        self.recorded = false;
    }

    pub(super) fn draw(&mut self) {
        let game = ArcadeGame::drawn(Some(self.game), self.dice.roll());
        self.show(game);
    }

    pub(super) fn next_game(&mut self) {
        self.show(self.game.next());
    }

    pub(super) fn play(&self) -> Play {
        self.board.play()
    }

    pub(super) fn points(&self) -> u32 {
        self.board.points()
    }

    pub(super) fn best(&self) -> u32 {
        self.scores.best(self.game)
    }

    pub(super) fn take(&mut self, steer: Steer) {
        if self.board.play() == Play::Playing {
            self.board.steer(steer);
            return;
        }
        self.board.restart(Play::Playing);
        self.recorded = false;
    }

    pub(super) fn aim(&mut self, column: f32) {
        if self.board.play() == Play::Playing {
            self.board.steer(Steer::Aim(column));
        }
    }

    pub(super) fn leave(&mut self) {
        self.board.restart(Play::Demo);
    }

    pub(super) fn step(&mut self, elapsed: Duration) -> bool {
        self.board.step(elapsed);
        if self.board.play() != Play::Crashed || self.recorded {
            return false;
        }
        self.recorded = true;
        self.scores.record(self.game, self.board.points())
    }

    pub(super) fn dots(&self) -> Vec<Dot> {
        let mut dots = Vec::new();
        self.board.dots(&mut dots);
        dots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(16);
    const FRAMES_PER_MINUTE: u32 = 3_750;

    fn cabinet(game: ArcadeGame) -> Cabinet {
        let mut cabinet = Cabinet::with_dice(Dice::seeded(9));
        cabinet.show(game);
        cabinet
    }

    #[test]
    fn a_new_chat_draws_a_different_game_than_the_one_shown() {
        let mut cabinet = Cabinet::with_dice(Dice::seeded(9));
        for _ in 0..20 {
            let shown = cabinet.game;
            cabinet.draw();
            assert_ne!(cabinet.game, shown);
            assert_eq!(cabinet.play(), Play::Demo);
        }
    }

    #[test]
    fn a_lost_run_records_its_best_once_and_a_new_run_starts_from_zero() {
        let mut cabinet = cabinet(ArcadeGame::ByteRun);
        cabinet.take(Steer::Action);
        assert_eq!(cabinet.play(), Play::Playing);
        let new_bests: usize = (0..FRAMES_PER_MINUTE)
            .map(|_| usize::from(cabinet.step(FRAME)))
            .sum();
        assert_eq!(cabinet.play(), Play::Crashed);
        assert_eq!(new_bests, 1);
        assert_eq!(cabinet.best(), cabinet.points());
        cabinet.take(Steer::Action);
        assert_eq!((cabinet.play(), cabinet.points()), (Play::Playing, 0));
    }

    #[test]
    fn the_demo_never_sets_a_best_score() {
        let mut cabinet = cabinet(ArcadeGame::TokenTrail);
        for _ in 0..FRAMES_PER_MINUTE {
            assert!(!cabinet.step(FRAME));
        }
        assert_eq!(cabinet.best(), 0);
    }

    #[test]
    fn leaving_or_switching_games_goes_back_to_the_demo() {
        let mut cabinet = cabinet(ArcadeGame::LineBreak);
        cabinet.take(Steer::Action);
        cabinet.leave();
        assert_eq!(cabinet.play(), Play::Demo);
        cabinet.take(Steer::Action);
        cabinet.next_game();
        assert_eq!(
            (cabinet.game, cabinet.play()),
            (ArcadeGame::TokenTrail, Play::Demo)
        );
    }
}
