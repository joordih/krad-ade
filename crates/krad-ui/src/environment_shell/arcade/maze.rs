mod carve;

use std::collections::VecDeque;
use std::time::Duration;

use self::carve::{carve, inside, MAZE_H, MAZE_W, START};
use super::board::{
    paint_cell, Board, Cell, Dice, Dot, Heading, Ink, Play, Steer, CELL_COUNT, MAX_STEP,
};

const HOME: Cell = Cell::new(MAZE_W / 2, MAZE_H / 2);
const PATCHES: [Cell; 4] = [
    Cell::new(1, 1),
    Cell::new(MAZE_W - 2, 1),
    Cell::new(MAZE_W - 2, MAZE_H - 2),
    Cell::new(MAZE_W / 2, MAZE_H - 2),
];
const CARET_PERIOD: f32 = 0.13;
const BUG_PERIOD: f32 = 0.18;
const MIN_BUG_PERIOD: f32 = 0.1;
const LEVEL_SPEEDUP: f32 = 0.9;
const FRIGHT_SLOWDOWN: f32 = 1.6;
const FRIGHT_TIME: f32 = 5.0;
const BUG_COUNT: usize = 3;
const MAX_BUGS: usize = 4;
const CHASE_BIAS: f32 = 0.55;
const PELLET_POINTS: u32 = 1;
const PATCH_POINTS: u32 = 5;
const BUG_POINTS: u32 = 10;
const SOLID: u8 = 0b1111;
const SPECK: u8 = 0b0001;
const DIAGONAL: u8 = 0b1001;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Food {
    Pellet,
    Patch,
}

#[derive(Clone, Copy, Debug)]
struct Chaser {
    cell: Cell,
    was: Cell,
    heading: Heading,
}

pub(super) struct Maze {
    play: Play,
    walls: Vec<bool>,
    food: Vec<Option<Food>>,
    caret: Cell,
    caret_was: Cell,
    heading: Option<Heading>,
    wanted: Option<Heading>,
    bugs: Vec<Chaser>,
    fright: f32,
    caret_clock: f32,
    bug_clock: f32,
    level: u32,
    points: u32,
    dice: Dice,
}

impl Maze {
    pub(super) fn new() -> Self {
        Self::with_dice(Dice::new())
    }

    fn with_dice(mut dice: Dice) -> Self {
        let walls = carve(&mut dice);
        let mut maze = Self {
            play: Play::Demo,
            food: Vec::new(),
            walls,
            caret: START,
            caret_was: START,
            heading: None,
            wanted: None,
            bugs: Vec::new(),
            fright: 0.0,
            caret_clock: 0.0,
            bug_clock: 0.0,
            level: 0,
            points: 0,
            dice,
        };
        maze.set_out();
        maze
    }

    fn set_out(&mut self) {
        self.food = (0..CELL_COUNT)
            .map(|index| (!self.walls[index]).then_some(Food::Pellet))
            .collect();
        for patch in PATCHES.iter().filter(|patch| !self.walls[patch.index()]) {
            self.food[patch.index()] = Some(Food::Patch);
        }
        self.food[START.index()] = None;
        self.walls[HOME.index()] = false;
        self.caret = START;
        self.caret_was = START;
        self.heading = None;
        self.wanted = None;
        let count = usize::try_from(self.level)
            .map_or(MAX_BUGS, |level| BUG_COUNT + level)
            .min(MAX_BUGS);
        self.bugs = (0..count)
            .map(|_| Chaser {
                cell: HOME,
                was: HOME,
                heading: Heading::Up,
            })
            .collect();
        self.fright = 0.0;
        self.caret_clock = 0.0;
        self.bug_clock = 0.0;
    }

    fn open(&self, cell: Cell) -> bool {
        inside(cell) && !self.walls[cell.index()]
    }

    fn bug_period(&self) -> f32 {
        let pace = (0..self.level).fold(BUG_PERIOD, |period, _| period * LEVEL_SPEEDUP);
        let pace = pace.max(MIN_BUG_PERIOD);
        if self.fright > 0.0 {
            pace * FRIGHT_SLOWDOWN
        } else {
            pace
        }
    }

    fn move_caret(&mut self) {
        if self.play == Play::Demo {
            self.wanted = self.autopilot();
        }
        if let Some(wanted) = self
            .wanted
            .filter(|wanted| self.open(self.caret.toward(*wanted)))
        {
            self.heading = Some(wanted);
        }
        self.caret_was = self.caret;
        let Some(heading) = self
            .heading
            .filter(|heading| self.open(self.caret.toward(*heading)))
        else {
            return;
        };
        self.caret = self.caret.toward(heading);
        match self.food[self.caret.index()].take() {
            Some(Food::Pellet) => self.points += PELLET_POINTS,
            Some(Food::Patch) => {
                self.points += PATCH_POINTS;
                self.fright = FRIGHT_TIME;
            }
            None => {}
        }
    }

    fn move_bugs(&mut self) {
        let caret = self.caret;
        let frightened = self.fright > 0.0;
        for index in 0..self.bugs.len() {
            let bug = self.bugs[index];
            let mut options: Vec<Heading> = Heading::ALL
                .into_iter()
                .filter(|heading| *heading != bug.heading.reverse())
                .filter(|heading| self.open(bug.cell.toward(*heading)))
                .collect();
            if options.is_empty() {
                options.push(bug.heading.reverse());
            }
            let distance = |heading: &Heading| bug.cell.toward(*heading).distance(caret);
            let heading = if frightened {
                options.iter().copied().max_by_key(distance)
            } else if self.dice.unit() < CHASE_BIAS {
                options.iter().copied().min_by_key(distance)
            } else {
                options.get(self.dice.below(options.len())).copied()
            };
            let Some(heading) = heading else {
                continue;
            };
            let target = bug.cell.toward(heading);
            let bug = &mut self.bugs[index];
            if inside(target) && !self.walls[target.index()] {
                bug.was = bug.cell;
                bug.cell = target;
                bug.heading = heading;
            }
        }
    }

    fn meet_bugs(&mut self) {
        let (caret, was) = (self.caret, self.caret_was);
        let met = |bug: &Chaser| bug.cell == caret || (bug.cell == was && bug.was == caret);
        if !self.bugs.iter().any(met) {
            return;
        }
        if self.fright <= 0.0 {
            match self.play {
                Play::Demo => self.restart(Play::Demo),
                Play::Playing | Play::Crashed => self.play = Play::Crashed,
            }
            return;
        }
        for bug in self.bugs.iter_mut().filter(|bug| met(bug)) {
            *bug = Chaser {
                cell: HOME,
                was: HOME,
                heading: Heading::Up,
            };
            self.points += BUG_POINTS;
        }
    }

    fn danger(&self) -> Vec<bool> {
        let mut danger = vec![false; CELL_COUNT];
        if self.fright > 0.0 {
            return danger;
        }
        for bug in &self.bugs {
            danger[bug.cell.index()] = true;
            for heading in Heading::ALL {
                let near = bug.cell.toward(heading);
                if inside(near) {
                    danger[near.index()] = true;
                }
            }
        }
        danger
    }

    fn autopilot(&self) -> Option<Heading> {
        let danger = self.danger();
        let mut first: Vec<Option<Heading>> = vec![None; CELL_COUNT];
        let mut seen = vec![false; CELL_COUNT];
        let mut queue = VecDeque::from([self.caret]);
        seen[self.caret.index()] = true;
        while let Some(at) = queue.pop_front() {
            if at != self.caret && self.food[at.index()].is_some() {
                return first[at.index()];
            }
            for heading in Heading::ALL {
                let next = at.toward(heading);
                if !self.open(next) || seen[next.index()] || danger[next.index()] {
                    continue;
                }
                seen[next.index()] = true;
                first[next.index()] = first[at.index()].or(Some(heading));
                queue.push_back(next);
            }
        }
        Heading::ALL
            .into_iter()
            .filter(|heading| self.open(self.caret.toward(*heading)))
            .max_by_key(|heading| {
                let next = self.caret.toward(*heading);
                self.bugs
                    .iter()
                    .map(|bug| bug.cell.distance(next))
                    .min()
                    .unwrap_or_default()
            })
    }

    fn cleared(&self) -> bool {
        self.food.iter().all(Option::is_none)
    }

    fn next_level(&mut self) {
        self.level += 1;
        self.walls = carve(&mut self.dice);
        self.set_out();
    }
}

impl Board for Maze {
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
        if let Steer::Turn(heading) = steer {
            self.wanted = Some(heading);
        }
    }

    fn step(&mut self, elapsed: Duration) {
        if self.play == Play::Crashed {
            return;
        }
        let dt = elapsed.min(MAX_STEP).as_secs_f32();
        self.fright = (self.fright - dt).max(0.0);
        self.caret_clock += dt;
        self.bug_clock += dt;
        while self.caret_clock >= CARET_PERIOD {
            self.caret_clock -= CARET_PERIOD;
            self.move_caret();
            self.meet_bugs();
            if self.play == Play::Crashed {
                return;
            }
        }
        let period = self.bug_period();
        while self.bug_clock >= period {
            self.bug_clock -= period;
            self.move_bugs();
            self.meet_bugs();
            if self.play == Play::Crashed {
                return;
            }
        }
        if self.cleared() {
            self.next_level();
        }
    }

    fn dots(&self, out: &mut Vec<Dot>) {
        let cells = (0..MAZE_H).flat_map(|y| (0..MAZE_W).map(move |x| Cell::new(x, y)));
        for cell in cells {
            if self.walls[cell.index()] {
                paint_cell(cell, SOLID, Ink::Faint, out);
            }
            match self.food[cell.index()] {
                Some(Food::Pellet) => paint_cell(cell, SPECK, Ink::Lit, out),
                Some(Food::Patch) => paint_cell(cell, DIAGONAL, Ink::Lit, out),
                None => {}
            }
        }
        let (look, ink) = if self.fright > 0.0 {
            (DIAGONAL, Ink::Faint)
        } else {
            (SOLID, Ink::Lit)
        };
        for bug in &self.bugs {
            paint_cell(bug.cell, look, ink, out);
        }
        paint_cell(self.caret, SOLID, Ink::Player, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(16);
    const FRAMES_PER_MINUTE: u32 = 3_750;

    fn maze() -> Maze {
        Maze::with_dice(Dice::seeded(11))
    }

    #[test]
    fn the_demo_eats_and_keeps_playing_for_a_minute() {
        let mut maze = maze();
        let mut most = 0;
        for _ in 0..FRAMES_PER_MINUTE {
            maze.step(FRAME);
            assert_eq!(maze.play, Play::Demo);
            most = most.max(maze.points);
        }
        assert!(most > 20, "the demo ate only {most}");
    }

    #[test]
    fn a_caret_left_still_is_caught_and_the_run_ends() {
        let mut maze = maze();
        maze.restart(Play::Playing);
        for _ in 0..FRAMES_PER_MINUTE {
            maze.step(FRAME);
        }
        assert_eq!(maze.play, Play::Crashed);
    }

    #[test]
    fn a_turn_into_a_wall_waits_until_the_corridor_opens() {
        let mut maze = maze();
        maze.restart(Play::Playing);
        maze.steer(Steer::Turn(Heading::Down));
        maze.move_caret();
        assert_eq!(maze.caret, START);
        assert_eq!(maze.wanted, Some(Heading::Down));
    }

    #[test]
    fn a_patch_lets_the_caret_eat_a_bug_and_sends_it_home() {
        let mut maze = maze();
        maze.restart(Play::Playing);
        maze.fright = FRIGHT_TIME;
        maze.bugs[0].cell = START;
        maze.meet_bugs();
        assert_eq!(maze.play, Play::Playing);
        assert_eq!((maze.bugs[0].cell, maze.points), (HOME, BUG_POINTS));
    }

    #[test]
    fn clearing_the_maze_starts_a_faster_level_and_keeps_the_points() {
        let mut maze = maze();
        maze.restart(Play::Playing);
        maze.points = 7;
        maze.food.iter_mut().for_each(|food| *food = None);
        maze.step(FRAME);
        assert_eq!((maze.level, maze.points, maze.play), (1, 7, Play::Playing));
        assert!(maze.bug_period() < BUG_PERIOD);
        assert!(!maze.cleared());
    }
}
