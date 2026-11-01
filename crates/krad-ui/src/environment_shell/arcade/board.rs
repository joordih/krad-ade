use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::time::Duration;

pub(super) const COLS: f32 = 96.0;
pub(super) const ROWS: f32 = 24.0;
pub(super) const CELLS_X: i32 = 48;
pub(super) const CELLS_Y: i32 = 12;
pub(super) const MAX_STEP: Duration = Duration::from_millis(50);
const CELL_DOTS: f32 = 2.0;
const RANDOM_BITS: u32 = 48;
const RANDOM_SPAN: f32 = 65_536.0;
const XORSHIFT: (u32, u32, u32) = (13, 7, 17);
const FALLBACK_SEED: u64 = 0x9e37_79b9_7f4a_7c15;
const BLOCK_CORNERS: [(f32, f32); 4] = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Play {
    Demo,
    Playing,
    Crashed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ink {
    Faint,
    Lit,
    Player,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Dot {
    pub(super) column: f32,
    pub(super) row: f32,
    pub(super) ink: Ink,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Heading {
    Up,
    Down,
    Left,
    Right,
}

impl Heading {
    pub(super) const ALL: [Self; 4] = [Self::Up, Self::Down, Self::Left, Self::Right];

    pub(super) const fn reverse(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Steer {
    Action,
    Turn(Heading),
    Aim(f32),
}

pub(super) trait Board {
    fn play(&self) -> Play;
    fn points(&self) -> u32;
    fn restart(&mut self, play: Play);
    fn steer(&mut self, steer: Steer);
    fn step(&mut self, elapsed: Duration);
    fn dots(&self, out: &mut Vec<Dot>);
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) struct Cell {
    pub(super) x: i32,
    pub(super) y: i32,
}

impl Cell {
    pub(super) const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub(super) const fn toward(self, heading: Heading) -> Self {
        match heading {
            Heading::Up => Self::new(self.x, self.y - 1),
            Heading::Down => Self::new(self.x, self.y + 1),
            Heading::Left => Self::new(self.x - 1, self.y),
            Heading::Right => Self::new(self.x + 1, self.y),
        }
    }

    pub(super) const fn on_board(self) -> bool {
        self.x >= 0 && self.y >= 0 && self.x < CELLS_X && self.y < CELLS_Y
    }

    pub(super) const fn index(self) -> usize {
        (self.y * CELLS_X + self.x).unsigned_abs() as usize
    }

    pub(super) const fn distance(self, other: Self) -> u32 {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }
}

pub(super) const CELL_COUNT: usize = (CELLS_X * CELLS_Y).unsigned_abs() as usize;

fn cell_dots(coordinate: i32) -> f32 {
    i16::try_from(coordinate).map_or_default(f32::from) * CELL_DOTS
}

pub(super) fn paint_cell(cell: Cell, corners: u8, ink: Ink, out: &mut Vec<Dot>) {
    let (left, top) = (cell_dots(cell.x), cell_dots(cell.y));
    out.extend(
        BLOCK_CORNERS
            .iter()
            .zip(0u8..)
            .filter(|(_, bit)| corners & (1 << bit) != 0)
            .map(|((dx, dy), _)| Dot {
                column: left + dx,
                row: top + dy,
                ink,
            }),
    );
}

pub(super) struct Dice(u64);

impl Dice {
    pub(super) fn new() -> Self {
        Self::seeded(RandomState::new().hash_one(RANDOM_BITS))
    }

    pub(super) const fn seeded(seed: u64) -> Self {
        Self(if seed == 0 { FALLBACK_SEED } else { seed })
    }

    pub(super) const fn roll(&mut self) -> u64 {
        let (a, b, c) = XORSHIFT;
        self.0 ^= self.0 << a;
        self.0 ^= self.0 >> b;
        self.0 ^= self.0 << c;
        self.0
    }

    pub(super) fn unit(&mut self) -> f32 {
        let high = u16::try_from(self.roll() >> RANDOM_BITS).unwrap_or_default();
        f32::from(high) / RANDOM_SPAN
    }

    pub(super) fn below(&mut self, count: usize) -> usize {
        let count = u64::try_from(count.max(1)).unwrap_or(1);
        usize::try_from(self.roll() % count).unwrap_or_default()
    }
}
