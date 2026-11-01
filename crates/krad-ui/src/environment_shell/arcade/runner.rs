use std::time::Duration;

use super::board::{Board, Dice, Dot, Heading, Ink, Play, Steer, COLS, MAX_STEP, ROWS};

const GROUND_ROW: f32 = ROWS - 2.0;
const RUNNER_COL: f32 = 8.0;
const RUNNER_W: f32 = 2.0;
const RUNNER_H: f32 = 3.0;
const GRAVITY: f32 = 170.0;
const JUMP_SPEED: f32 = 42.0;
const START_SPEED: f32 = 26.0;
const TOP_SPEED: f32 = 62.0;
const SPEEDUP: f32 = 0.8;
const MIN_GAP: f32 = 22.0;
const GAP_SPREAD: f32 = 26.0;
const GAP_PER_SPEED: f32 = 0.25;
const DEMO_LEAD: f32 = 0.16;
const HIT_SLACK: f32 = 0.25;
const DOTS_PER_POINT: f32 = 4.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bug {
    Crawler,
    Spike,
    Block,
}

impl Bug {
    const fn rows(self) -> &'static [u8] {
        match self {
            Self::Crawler => &[0b101, 0b111],
            Self::Spike => &[0b1, 0b1, 0b1],
            Self::Block => &[0b11, 0b11],
        }
    }

    const fn width(self) -> f32 {
        match self {
            Self::Crawler => 3.0,
            Self::Spike => 1.0,
            Self::Block => 2.0,
        }
    }

    fn dots(self) -> impl Iterator<Item = (f32, f32)> {
        self.rows().iter().zip(0u8..).flat_map(|(mask, height)| {
            (0u8..8)
                .filter(move |column| mask & (1 << column) != 0)
                .map(move |column| (f32::from(column), f32::from(height)))
        })
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Obstacle {
    x: f32,
    bug: Bug,
}

pub(super) struct Runner {
    play: Play,
    height: f32,
    velocity: f32,
    obstacles: Vec<Obstacle>,
    speed: f32,
    until_next: f32,
    run: f32,
    points: u32,
    dice: Dice,
}

impl Runner {
    pub(super) fn new() -> Self {
        Self::with_dice(Dice::new())
    }

    fn with_dice(dice: Dice) -> Self {
        Self {
            play: Play::Demo,
            height: 0.0,
            velocity: 0.0,
            obstacles: Vec::new(),
            speed: START_SPEED,
            until_next: MIN_GAP,
            run: 0.0,
            points: 0,
            dice,
        }
    }

    fn jump(&mut self) {
        if self.on_ground() {
            self.velocity = JUMP_SPEED;
        }
    }

    fn on_ground(&self) -> bool {
        self.height <= 0.0 && self.velocity <= 0.0
    }

    fn bug_ahead(&self) -> bool {
        let reach = self.speed * DEMO_LEAD + 1.0;
        self.obstacles.iter().any(|obstacle| {
            let gap = obstacle.x - (RUNNER_COL + RUNNER_W);
            (0.0..reach).contains(&gap)
        })
    }

    fn hit(&self) -> bool {
        let (left, right) = (RUNNER_COL + HIT_SLACK, RUNNER_COL + RUNNER_W - HIT_SLACK);
        let (bottom, top) = (self.height + HIT_SLACK, self.height + RUNNER_H - HIT_SLACK);
        self.obstacles.iter().any(|obstacle| {
            obstacle.bug.dots().any(|(column, height)| {
                let x = obstacle.x + column;
                x < right && x + 1.0 > left && height < top && height + 1.0 > bottom
            })
        })
    }

    fn next_bug(&mut self) -> Bug {
        match self.dice.below(3) {
            0 => Bug::Crawler,
            1 => Bug::Spike,
            _ => Bug::Block,
        }
    }
}

impl Board for Runner {
    fn play(&self) -> Play {
        self.play
    }

    fn points(&self) -> u32 {
        self.points
    }

    fn restart(&mut self, play: Play) {
        let dice = std::mem::replace(&mut self.dice, Dice::seeded(0));
        *self = Self::with_dice(dice);
        self.play = play;
    }

    fn steer(&mut self, steer: Steer) {
        if matches!(steer, Steer::Action | Steer::Turn(Heading::Up)) {
            self.jump();
        }
    }

    fn step(&mut self, elapsed: Duration) {
        if self.play == Play::Crashed {
            return;
        }
        let dt = elapsed.min(MAX_STEP).as_secs_f32();
        if !self.on_ground() {
            self.velocity -= GRAVITY * dt;
            self.height = (self.height + self.velocity * dt).max(0.0);
            if self.height <= 0.0 {
                self.velocity = 0.0;
            }
        }
        self.speed = (self.speed + SPEEDUP * dt).min(TOP_SPEED);
        let travelled = self.speed * dt;
        for obstacle in &mut self.obstacles {
            obstacle.x -= travelled;
        }
        self.obstacles
            .retain(|obstacle| obstacle.x + obstacle.bug.width() > 0.0);
        self.until_next -= travelled;
        if self.until_next <= 0.0 {
            let bug = self.next_bug();
            self.obstacles.push(Obstacle { x: COLS, bug });
            self.until_next = MIN_GAP + GAP_SPREAD * self.dice.unit() + self.speed * GAP_PER_SPEED;
        }
        if self.play == Play::Demo && self.bug_ahead() {
            self.jump();
        }
        if self.hit() {
            match self.play {
                Play::Demo => self.restart(Play::Demo),
                Play::Playing | Play::Crashed => self.play = Play::Crashed,
            }
            return;
        }
        self.run += travelled;
        while self.run >= DOTS_PER_POINT {
            self.run -= DOTS_PER_POINT;
            self.points += 1;
        }
    }

    fn dots(&self, out: &mut Vec<Dot>) {
        let ground = (0u8..)
            .map(f32::from)
            .take_while(|column| *column < COLS)
            .map(|column| Dot {
                column,
                row: GROUND_ROW,
                ink: Ink::Lit,
            });
        let feet = GROUND_ROW - 1.0 - self.height.round();
        let caret = [0.0, 1.0].into_iter().flat_map(move |dx| {
            [0.0, 1.0, 2.0].into_iter().map(move |dy| Dot {
                column: RUNNER_COL + dx,
                row: feet - dy,
                ink: Ink::Player,
            })
        });
        let bugs = self.obstacles.iter().flat_map(|obstacle| {
            let left = obstacle.x.floor();
            obstacle.bug.dots().map(move |(column, height)| Dot {
                column: left + column,
                row: GROUND_ROW - 1.0 - height,
                ink: Ink::Lit,
            })
        });
        out.extend(
            ground
                .chain(caret)
                .chain(bugs.filter(|dot| (0.0..COLS).contains(&dot.column))),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(16);
    const FRAMES_PER_MINUTE: u32 = 3_750;

    fn runner() -> Runner {
        Runner::with_dice(Dice::seeded(7))
    }

    #[test]
    fn the_demo_jumps_every_bug_for_a_minute() {
        let mut runner = runner();
        for _ in 0..FRAMES_PER_MINUTE {
            runner.step(FRAME);
            assert_eq!(runner.play, Play::Demo);
        }
        assert!(runner.points > 0, "the demo restarted after a hit");
    }

    #[test]
    fn a_player_who_never_jumps_crashes_and_a_new_run_starts_from_zero() {
        let mut runner = runner();
        runner.restart(Play::Playing);
        for _ in 0..FRAMES_PER_MINUTE {
            runner.step(FRAME);
        }
        assert_eq!(runner.play, Play::Crashed);
        assert!(runner.points > 0);
        runner.restart(Play::Playing);
        assert_eq!((runner.play, runner.points), (Play::Playing, 0));
    }

    #[test]
    fn the_caret_jumps_only_from_the_ground() {
        let mut runner = runner();
        runner.restart(Play::Playing);
        runner.steer(Steer::Action);
        runner.step(FRAME);
        assert!(runner.height > 0.0);
        runner.steer(Steer::Turn(Heading::Up));
        runner.step(FRAME);
        assert!(runner.velocity < JUMP_SPEED, "a second jump in the air");
    }

    #[test]
    fn releasing_the_caret_lands_it_back_in_the_demo() {
        let mut runner = runner();
        runner.restart(Play::Playing);
        runner.steer(Steer::Action);
        runner.step(FRAME);
        runner.restart(Play::Demo);
        assert_eq!((runner.play, runner.height), (Play::Demo, 0.0));
    }

    #[test]
    fn a_crawler_has_its_legs_below() {
        let dots: Vec<(f32, f32)> = Bug::Crawler.dots().collect();
        assert_eq!(dots.len(), 5);
        assert!(!dots.contains(&(1.0, 0.0)) && dots.contains(&(1.0, 1.0)));
    }

    #[test]
    fn the_ground_lies_on_the_bottom_rows_of_the_taller_banner() {
        let mut dots = Vec::new();
        runner().dots(&mut dots);
        assert!(dots
            .iter()
            .all(|dot| (0.0..ROWS).contains(&dot.row) && (0.0..COLS).contains(&dot.column)));
        assert_eq!(dots.iter().filter(|dot| dot.row == ROWS - 2.0).count(), 96);
    }
}
