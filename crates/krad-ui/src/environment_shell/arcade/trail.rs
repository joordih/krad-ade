use std::collections::VecDeque;
use std::time::Duration;

use super::board::{
    paint_cell, Board, Cell, Dice, Dot, Heading, Ink, Play, Steer, CELLS_X, CELLS_Y, CELL_COUNT,
    MAX_STEP,
};

const START_LENGTH: i32 = 4;
const START_ROW: i32 = CELLS_Y / 2;
const START_PERIOD: f32 = 0.12;
const MIN_PERIOD: f32 = 0.055;
const SPEEDUP_PER_TOKEN: f32 = 0.985;
const GROWTH: u32 = 2;
const QUEUED_TURNS: usize = 2;
const SOLID: u8 = 0b1111;
const DIAGONAL: u8 = 0b1001;

pub(super) struct Trail {
    play: Play,
    body: VecDeque<Cell>,
    heading: Heading,
    turns: VecDeque<Heading>,
    growing: u32,
    token: Option<Cell>,
    clock: f32,
    points: u32,
    dice: Dice,
}

impl Trail {
    pub(super) fn new() -> Self {
        Self::with_dice(Dice::new())
    }

    fn with_dice(dice: Dice) -> Self {
        let body = (1..=START_LENGTH)
            .rev()
            .map(|x| Cell::new(x, START_ROW))
            .collect();
        let mut trail = Self {
            play: Play::Demo,
            body,
            heading: Heading::Right,
            turns: VecDeque::new(),
            growing: 0,
            token: None,
            clock: 0.0,
            points: 0,
            dice,
        };
        trail.place_token();
        trail
    }

    fn head(&self) -> Cell {
        self.body
            .front()
            .copied()
            .unwrap_or(Cell::new(0, START_ROW))
    }

    fn occupied(&self) -> Vec<bool> {
        let mut occupied = vec![false; CELL_COUNT];
        let keeps_tail = self.growing > 0;
        let len = self.body.len();
        for (at, cell) in self.body.iter().enumerate() {
            if keeps_tail || at + 1 < len {
                occupied[cell.index()] = true;
            }
        }
        occupied
    }

    fn place_token(&mut self) {
        let mut taken = vec![false; CELL_COUNT];
        for cell in &self.body {
            taken[cell.index()] = true;
        }
        let free: Vec<Cell> = (0..CELLS_Y)
            .flat_map(|y| (0..CELLS_X).map(move |x| Cell::new(x, y)))
            .filter(|cell| !taken[cell.index()])
            .collect();
        self.token = free.get(self.dice.below(free.len())).copied();
    }

    fn period(&self) -> f32 {
        (0..self.points)
            .fold(START_PERIOD, |period, _| period * SPEEDUP_PER_TOKEN)
            .max(MIN_PERIOD)
    }

    fn room_after(&self, heading: Heading, occupied: &[bool]) -> usize {
        let start = self.head().toward(heading);
        if !start.on_board() || occupied[start.index()] {
            return 0;
        }
        let mut seen = vec![false; CELL_COUNT];
        let mut queue = VecDeque::from([start]);
        seen[start.index()] = true;
        let mut room = 0;
        while let Some(at) = queue.pop_front() {
            room += 1;
            for next in Heading::ALL.map(|heading| at.toward(heading)) {
                if next.on_board() && !occupied[next.index()] && !seen[next.index()] {
                    seen[next.index()] = true;
                    queue.push_back(next);
                }
            }
        }
        room
    }

    fn path_to_token(&self, occupied: &[bool]) -> Option<Heading> {
        let token = self.token?;
        let head = self.head();
        let mut first: Vec<Option<Heading>> = vec![None; CELL_COUNT];
        let mut seen = vec![false; CELL_COUNT];
        let mut queue = VecDeque::from([head]);
        seen[head.index()] = true;
        while let Some(at) = queue.pop_front() {
            if at == token {
                return first[at.index()];
            }
            for heading in Heading::ALL {
                let next = at.toward(heading);
                if !next.on_board() || occupied[next.index()] || seen[next.index()] {
                    continue;
                }
                seen[next.index()] = true;
                first[next.index()] = first[at.index()].or(Some(heading));
                queue.push_back(next);
            }
        }
        None
    }

    fn autopilot(&self) -> Heading {
        let occupied = self.occupied();
        let length = self.body.len();
        if let Some(heading) = self
            .path_to_token(&occupied)
            .filter(|heading| self.room_after(*heading, &occupied) > length)
        {
            return heading;
        }
        Heading::ALL
            .into_iter()
            .filter(|heading| *heading != self.heading.reverse())
            .max_by_key(|heading| self.room_after(*heading, &occupied))
            .unwrap_or(self.heading)
    }

    fn advance(&mut self) {
        if self.play == Play::Demo {
            self.heading = self.autopilot();
        } else if let Some(turn) = self.turns.pop_front() {
            self.heading = turn;
        }
        let next = self.head().toward(self.heading);
        if !next.on_board() || self.occupied()[next.index()] {
            match self.play {
                Play::Demo => self.restart(Play::Demo),
                Play::Playing | Play::Crashed => self.play = Play::Crashed,
            }
            return;
        }
        self.body.push_front(next);
        if Some(next) == self.token {
            self.points += 1;
            self.growing += GROWTH;
            self.place_token();
        }
        if self.growing > 0 {
            self.growing -= 1;
        } else {
            self.body.pop_back();
        }
        if self.token.is_none() {
            let points = self.points;
            self.restart(self.play);
            self.points = points;
        }
    }
}

impl Board for Trail {
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
        let Steer::Turn(heading) = steer else {
            return;
        };
        let last = self.turns.back().copied().unwrap_or(self.heading);
        if heading == last || heading == last.reverse() || self.turns.len() >= QUEUED_TURNS {
            return;
        }
        self.turns.push_back(heading);
    }

    fn step(&mut self, elapsed: Duration) {
        if self.play == Play::Crashed {
            return;
        }
        self.clock += elapsed.min(MAX_STEP).as_secs_f32();
        let mut period = self.period();
        while self.clock >= period && self.play != Play::Crashed {
            self.clock -= period;
            self.advance();
            period = self.period();
        }
    }

    fn dots(&self, out: &mut Vec<Dot>) {
        for cell in &self.body {
            paint_cell(*cell, SOLID, Ink::Player, out);
        }
        if let Some(token) = self.token {
            paint_cell(token, DIAGONAL, Ink::Lit, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(16);
    const FRAMES_PER_MINUTE: u32 = 3_750;

    fn trail() -> Trail {
        Trail::with_dice(Dice::seeded(5))
    }

    #[test]
    fn the_demo_collects_tokens_for_a_minute() {
        let mut trail = trail();
        let mut most = 0;
        for _ in 0..FRAMES_PER_MINUTE {
            trail.step(FRAME);
            assert_eq!(trail.play, Play::Demo);
            most = most.max(trail.points);
        }
        assert!(most >= 10, "the demo collected only {most}");
    }

    #[test]
    fn a_trail_left_on_its_heading_hits_the_edge_and_the_run_ends() {
        let mut trail = trail();
        trail.restart(Play::Playing);
        trail.token = Some(Cell::new(0, 0));
        for _ in 0..FRAMES_PER_MINUTE {
            trail.step(FRAME);
        }
        assert_eq!(trail.play, Play::Crashed);
    }

    #[test]
    fn a_token_adds_a_point_and_grows_the_trail() {
        let mut trail = trail();
        trail.restart(Play::Playing);
        trail.token = Some(trail.head().toward(Heading::Right));
        let length = trail.body.len();
        trail.advance();
        trail.advance();
        trail.advance();
        assert_eq!(trail.points, 1);
        assert_eq!(trail.body.len(), length + 2);
    }

    #[test]
    fn a_turn_back_onto_itself_is_ignored_and_at_most_two_turns_wait() {
        let mut trail = trail();
        trail.restart(Play::Playing);
        trail.steer(Steer::Turn(Heading::Left));
        assert!(trail.turns.is_empty());
        trail.steer(Steer::Turn(Heading::Up));
        trail.steer(Steer::Turn(Heading::Left));
        trail.steer(Steer::Turn(Heading::Down));
        assert_eq!(trail.turns, [Heading::Up, Heading::Left]);
    }
}
