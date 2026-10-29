use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::time::Duration;

pub(super) const COLS: f32 = 96.0;
pub(super) const ROWS: f32 = 14.0;
pub(super) const GROUND_ROW: f32 = 12.0;
pub(super) const RUNNER_COL: f32 = 8.0;
pub(super) const RUNNER_W: f32 = 2.0;
pub(super) const RUNNER_H: f32 = 3.0;
const GRAVITY: f32 = 170.0;
const JUMP_SPEED: f32 = 42.0;
const START_SPEED: f32 = 26.0;
const TOP_SPEED: f32 = 62.0;
const SPEEDUP: f32 = 0.8;
const MIN_GAP: f32 = 22.0;
const GAP_SPREAD: f32 = 26.0;
const GAP_PER_SPEED: f32 = 0.25;
const MAX_STEP: Duration = Duration::from_millis(50);
const DEMO_LEAD: f32 = 0.16;
const HIT_SLACK: f32 = 0.25;
const DOTS_PER_POINT: f32 = 4.0;
const RANDOM_BITS: u32 = 48;
const RANDOM_SPAN: f32 = 65_536.0;
const XORSHIFT: (u32, u32, u32) = (13, 7, 17);
const FALLBACK_SEED: u64 = 0x9e37_79b9_7f4a_7c15;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Play {
    Demo,
    Playing,
    Crashed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Bug {
    Crawler,
    Spike,
    Block,
}

impl Bug {
    pub(super) const fn rows(self) -> &'static [u8] {
        match self {
            Self::Crawler => &[0b101, 0b111],
            Self::Spike => &[0b1, 0b1, 0b1],
            Self::Block => &[0b11, 0b11],
        }
    }

    pub(super) const fn width(self) -> f32 {
        match self {
            Self::Crawler => 3.0,
            Self::Spike => 1.0,
            Self::Block => 2.0,
        }
    }

    pub(super) fn dots(self) -> impl Iterator<Item = (f32, f32)> {
        self.rows().iter().zip(0u8..).flat_map(|(mask, height)| {
            (0u8..8)
                .filter(move |column| mask & (1 << column) != 0)
                .map(move |column| (f32::from(column), f32::from(height)))
        })
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Obstacle {
    pub(super) x: f32,
    pub(super) bug: Bug,
}

pub(super) struct Runner {
    pub(super) play: Play,
    pub(super) height: f32,
    velocity: f32,
    pub(super) obstacles: Vec<Obstacle>,
    speed: f32,
    until_next: f32,
    run: f32,
    pub(super) points: u32,
    pub(super) best: u32,
    seed: u64,
}

impl Runner {
    pub(super) fn new() -> Self {
        let seed = RandomState::new().hash_one(RANDOM_BITS);
        Self::seeded(seed)
    }

    fn seeded(seed: u64) -> Self {
        Self {
            play: Play::Demo,
            height: 0.0,
            velocity: 0.0,
            obstacles: Vec::new(),
            speed: START_SPEED,
            until_next: MIN_GAP,
            run: 0.0,
            points: 0,
            best: 0,
            seed: if seed == 0 { FALLBACK_SEED } else { seed },
        }
    }

    fn restart(&mut self, play: Play) {
        let best = self.best;
        let seed = self.seed;
        *self = Self::seeded(seed);
        self.best = best;
        self.play = play;
    }

    pub(super) fn press(&mut self) {
        match self.play {
            Play::Demo | Play::Crashed => self.restart(Play::Playing),
            Play::Playing => self.jump(),
        }
    }

    pub(super) fn release(&mut self) {
        self.restart(Play::Demo);
    }

    fn jump(&mut self) {
        if self.on_ground() {
            self.velocity = JUMP_SPEED;
        }
    }

    fn on_ground(&self) -> bool {
        self.height <= 0.0 && self.velocity <= 0.0
    }

    pub(super) fn step(&mut self, elapsed: Duration) {
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
            self.until_next = MIN_GAP + GAP_SPREAD * self.next_unit() + self.speed * GAP_PER_SPEED;
        }
        if self.play == Play::Demo && self.bug_ahead() {
            self.jump();
        }
        if self.hit() {
            match self.play {
                Play::Demo => self.restart(Play::Demo),
                Play::Playing | Play::Crashed => {
                    self.play = Play::Crashed;
                    self.best = self.best.max(self.points);
                }
            }
            return;
        }
        self.run += travelled;
        while self.run >= DOTS_PER_POINT {
            self.run -= DOTS_PER_POINT;
            self.points += 1;
        }
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

    fn next_random(&mut self) -> u64 {
        let (a, b, c) = XORSHIFT;
        self.seed ^= self.seed << a;
        self.seed ^= self.seed >> b;
        self.seed ^= self.seed << c;
        self.seed
    }

    fn next_unit(&mut self) -> f32 {
        let high = u16::try_from(self.next_random() >> RANDOM_BITS).unwrap_or_default();
        f32::from(high) / RANDOM_SPAN
    }

    fn next_bug(&mut self) -> Bug {
        match self.next_random() % 3 {
            0 => Bug::Crawler,
            1 => Bug::Spike,
            _ => Bug::Block,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(16);
    const FRAMES_PER_MINUTE: u32 = 3_750;

    #[test]
    fn the_demo_jumps_every_bug_for_a_minute() {
        let mut runner = Runner::seeded(7);
        for _ in 0..FRAMES_PER_MINUTE {
            runner.step(FRAME);
            assert_eq!(runner.play, Play::Demo);
        }
        assert!(runner.points > 0, "the demo restarted after a hit");
    }

    #[test]
    fn a_player_who_never_jumps_crashes_and_sets_the_best() {
        let mut runner = Runner::seeded(7);
        runner.press();
        assert_eq!(runner.play, Play::Playing);
        for _ in 0..FRAMES_PER_MINUTE {
            runner.step(FRAME);
        }
        assert_eq!(runner.play, Play::Crashed);
        assert_eq!(runner.best, runner.points);
        let best = runner.best;
        runner.press();
        assert_eq!(
            (runner.play, runner.points, runner.best),
            (Play::Playing, 0, best)
        );
    }

    #[test]
    fn the_caret_jumps_only_from_the_ground() {
        let mut runner = Runner::seeded(7);
        runner.press();
        runner.press();
        runner.step(FRAME);
        let rising = runner.height;
        assert!(rising > 0.0);
        runner.press();
        runner.step(FRAME);
        assert!(runner.velocity < JUMP_SPEED, "a second jump in the air");
        runner.release();
        assert_eq!((runner.play, runner.height), (Play::Demo, 0.0));
    }

    #[test]
    fn a_crawler_has_its_legs_below() {
        let dots: Vec<(f32, f32)> = Bug::Crawler.dots().collect();
        assert_eq!(dots.len(), 5);
        assert!(!dots.contains(&(1.0, 0.0)) && dots.contains(&(1.0, 1.0)));
    }
}
