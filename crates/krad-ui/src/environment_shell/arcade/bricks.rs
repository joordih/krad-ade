use std::time::Duration;

use super::board::{Board, Dice, Dot, Heading, Ink, Play, Steer, COLS, MAX_STEP, ROWS};

const FIRST_LINE: u8 = 2;
const LINES: u8 = 8;
const INDENT_STEP: u8 = 2;
const MAX_INDENT_STEPS: usize = 4;
const RIGHT_MARGIN: u8 = 4;
const MIN_TOKEN: u8 = 2;
const TOKEN_SPREAD: usize = 6;
const TOKEN_GAP: u8 = 1;
const PADDLE_ROW: f32 = ROWS - 2.0;
const PADDLE_W: f32 = 12.0;
const PADDLE_SPEED: f32 = 90.0;
const PADDLE_NUDGE: f32 = 8.0;
const START_SPEED: f32 = 28.0;
const LEVEL_SPEEDUP: f32 = 4.0;
const TOP_SPEED: f32 = 60.0;
const SUBSTEP: f32 = 0.5;
const LAUNCH_SLANT: f32 = 0.5;
const MAX_SLANT: f32 = 0.8;
const DEMO_AIM_SPREAD: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Brick {
    row: u8,
    left: u8,
    width: u8,
}

impl Brick {
    fn covers(self, column: f32, row: f32) -> bool {
        let (left, width, top) = (
            f32::from(self.left),
            f32::from(self.width),
            f32::from(self.row),
        );
        (top..top + 1.0).contains(&row) && (left..left + width).contains(&column)
    }
}

pub(super) struct Bricks {
    play: Play,
    wall: Vec<Brick>,
    paddle: f32,
    target: f32,
    ball: (f32, f32),
    velocity: (f32, f32),
    demo_aim: f32,
    level: u32,
    points: u32,
    dice: Dice,
}

fn build_wall(dice: &mut Dice) -> Vec<Brick> {
    let mut bricks = Vec::new();
    let right = COLS - f32::from(RIGHT_MARGIN);
    for row in FIRST_LINE..FIRST_LINE + LINES {
        let indent = u8::try_from(dice.below(MAX_INDENT_STEPS)).unwrap_or_default() * INDENT_STEP;
        let mut left = indent;
        loop {
            let width = MIN_TOKEN + u8::try_from(dice.below(TOKEN_SPREAD)).unwrap_or_default();
            if f32::from(left) + f32::from(width) > right {
                break;
            }
            bricks.push(Brick { row, left, width });
            left += width + TOKEN_GAP;
        }
    }
    bricks
}

impl Bricks {
    pub(super) fn new() -> Self {
        Self::with_dice(Dice::new())
    }

    fn with_dice(mut dice: Dice) -> Self {
        let wall = build_wall(&mut dice);
        let mut game = Self {
            play: Play::Demo,
            wall,
            paddle: (COLS - PADDLE_W) / 2.0,
            target: (COLS - PADDLE_W) / 2.0,
            ball: (0.0, 0.0),
            velocity: (0.0, 0.0),
            demo_aim: 0.0,
            level: 0,
            points: 0,
            dice,
        };
        game.serve();
        game
    }

    fn speed(&self) -> f32 {
        let level = u16::try_from(self.level).map_or(TOP_SPEED, f32::from);
        (START_SPEED + LEVEL_SPEEDUP * level).min(TOP_SPEED)
    }

    fn serve(&mut self) {
        let speed = self.speed();
        let side = if self.dice.below(2) == 0 { -1.0 } else { 1.0 };
        let across = speed * LAUNCH_SLANT * side;
        self.ball = (self.paddle + PADDLE_W / 2.0, PADDLE_ROW - 1.0);
        self.velocity = (across, -(speed * speed - across * across).sqrt());
    }

    fn move_paddle(&mut self, dt: f32) {
        if self.play == Play::Demo {
            self.target = self.ball.0 - PADDLE_W / 2.0 + self.demo_aim;
        }
        self.target = self.target.clamp(0.0, COLS - PADDLE_W);
        let reach = PADDLE_SPEED * dt;
        self.paddle += (self.target - self.paddle).clamp(-reach, reach);
    }

    fn bounce_off_paddle(&mut self, column: f32) {
        let center = self.paddle + PADDLE_W / 2.0;
        let slant = ((column - center) / (PADDLE_W / 2.0)).clamp(-MAX_SLANT, MAX_SLANT);
        let speed = self.speed();
        let across = speed * slant;
        self.velocity = (across, -(speed * speed - across * across).sqrt());
        self.demo_aim = (self.dice.unit() - 0.5) * DEMO_AIM_SPREAD;
    }

    fn lose(&mut self) {
        match self.play {
            Play::Demo => self.restart(Play::Demo),
            Play::Playing | Play::Crashed => self.play = Play::Crashed,
        }
    }

    fn move_ball(&mut self, dt: f32) {
        let distance = self.speed() * dt;
        let steps = (distance / SUBSTEP).ceil().max(1.0);
        let slice = dt / steps;
        let mut left = steps;
        while left > 0.0 {
            left -= 1.0;
            let (x, y) = self.ball;
            let (mut vx, mut vy) = self.velocity;
            let (mut nx, mut ny) = (x + vx * slice, y + vy * slice);
            if nx < 0.0 {
                nx = -nx;
                vx = vx.abs();
            } else if nx >= COLS {
                nx = 2.0 * COLS - nx - SUBSTEP;
                vx = -vx.abs();
            }
            if ny < 0.0 {
                ny = -ny;
                vy = vy.abs();
            }
            if let Some(hit) = self.wall.iter().position(|brick| brick.covers(nx, ny)) {
                let brick = self.wall.swap_remove(hit);
                self.points += 1;
                if brick.covers(nx, y) {
                    vx = -vx;
                } else {
                    vy = -vy;
                }
                self.velocity = (vx, vy);
                continue;
            }
            self.velocity = (vx, vy);
            let paddle = self.paddle - SUBSTEP..=self.paddle + PADDLE_W + SUBSTEP;
            if vy > 0.0 && y < PADDLE_ROW && ny >= PADDLE_ROW && paddle.contains(&nx) {
                self.bounce_off_paddle(nx);
                continue;
            }
            if ny >= ROWS {
                self.lose();
                return;
            }
            self.ball = (nx, ny);
        }
    }

    fn next_level(&mut self) {
        self.level += 1;
        self.wall = build_wall(&mut self.dice);
        self.serve();
    }
}

impl Board for Bricks {
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
        match steer {
            Steer::Turn(Heading::Left) => self.target = self.paddle - PADDLE_NUDGE,
            Steer::Turn(Heading::Right) => self.target = self.paddle + PADDLE_NUDGE,
            Steer::Aim(column) => self.target = column - PADDLE_W / 2.0,
            Steer::Action | Steer::Turn(Heading::Up | Heading::Down) => {}
        }
    }

    fn step(&mut self, elapsed: Duration) {
        if self.play == Play::Crashed {
            return;
        }
        let dt = elapsed.min(MAX_STEP).as_secs_f32();
        self.move_paddle(dt);
        self.move_ball(dt);
        if self.play != Play::Crashed && self.wall.is_empty() {
            self.next_level();
        }
    }

    fn dots(&self, out: &mut Vec<Dot>) {
        for brick in &self.wall {
            out.extend((0..brick.width).map(|offset| Dot {
                column: f32::from(brick.left + offset),
                row: f32::from(brick.row),
                ink: Ink::Lit,
            }));
        }
        let paddle = self.paddle.round();
        out.extend(
            (0u8..)
                .map(f32::from)
                .take_while(|offset| *offset < PADDLE_W)
                .map(|offset| Dot {
                    column: paddle + offset,
                    row: PADDLE_ROW,
                    ink: Ink::Player,
                }),
        );
        out.push(Dot {
            column: self.ball.0.floor(),
            row: self.ball.1.floor(),
            ink: Ink::Player,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(16);
    const FRAMES_PER_MINUTE: u32 = 3_750;

    fn bricks() -> Bricks {
        Bricks::with_dice(Dice::seeded(3))
    }

    #[test]
    fn every_line_of_the_wall_fits_inside_the_banner() {
        let game = bricks();
        assert_ne!(game.wall.len(), 0);
        assert!(game.wall.iter().all(|brick| {
            f32::from(brick.left + brick.width) <= COLS && f32::from(brick.row) < PADDLE_ROW
        }));
    }

    #[test]
    fn the_demo_breaks_bricks_and_keeps_the_ball_for_a_minute() {
        let mut game = bricks();
        let mut most = 0;
        for _ in 0..FRAMES_PER_MINUTE {
            game.step(FRAME);
            assert_eq!(game.play, Play::Demo);
            most = most.max(game.points);
        }
        assert!(most > 10, "the demo broke only {most}");
    }

    #[test]
    fn a_paddle_left_at_the_edge_drops_the_ball_and_the_run_ends() {
        let mut game = bricks();
        game.restart(Play::Playing);
        game.steer(Steer::Aim(0.0));
        for _ in 0..FRAMES_PER_MINUTE {
            game.steer(Steer::Aim(0.0));
            game.step(FRAME);
        }
        assert_eq!(game.play, Play::Crashed);
    }

    #[test]
    fn the_ball_leaves_the_paddle_slanted_toward_the_side_it_hit() {
        let mut game = bricks();
        let center = game.paddle + PADDLE_W / 2.0;
        game.bounce_off_paddle(center + PADDLE_W / 2.0);
        assert!(game.velocity.0 > 0.0 && game.velocity.1 < 0.0);
        game.bounce_off_paddle(center - PADDLE_W / 2.0);
        assert!(game.velocity.0 < 0.0 && game.velocity.1 < 0.0);
    }

    #[test]
    fn clearing_the_wall_builds_a_new_one_and_speeds_the_ball_up() {
        let mut game = bricks();
        game.restart(Play::Playing);
        game.wall.clear();
        game.points = 4;
        game.step(FRAME);
        assert_eq!((game.level, game.points), (1, 4));
        assert_ne!(game.wall.len(), 0);
        assert!(game.speed() > START_SPEED);
    }
}
