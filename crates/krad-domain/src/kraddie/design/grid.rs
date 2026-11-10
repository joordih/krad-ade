use serde::{Deserialize, Serialize};
use std::fmt;

pub const CANVAS_SIZE: u8 = 24;
const PALETTE_SLOTS: u8 = 16;
pub const MAX_PALETTE: usize = PALETTE_SLOTS as usize;
const CELLS: usize = CANVAS_SIZE as usize * CANVAS_SIZE as usize;
const EMPTY_MARK: char = '.';
const INDEX_RADIX: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct PaletteIndex(u8);

impl PaletteIndex {
    pub const FIRST: Self = Self(0);

    pub(super) const fn wrapping(index: u8) -> Self {
        Self(index % PALETTE_SLOTS)
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0 as usize
    }
}

impl TryFrom<u8> for PaletteIndex {
    type Error = PixelGridError;

    fn try_from(index: u8) -> Result<Self, Self::Error> {
        (usize::from(index) < MAX_PALETTE)
            .then_some(Self(index))
            .ok_or(PixelGridError::BadPixel)
    }
}

impl TryFrom<usize> for PaletteIndex {
    type Error = PixelGridError;

    fn try_from(index: usize) -> Result<Self, Self::Error> {
        u8::try_from(index)
            .map_err(|_| PixelGridError::BadPixel)
            .and_then(Self::try_from)
    }
}

impl From<PaletteIndex> for u8 {
    fn from(index: PaletteIndex) -> Self {
        index.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CanvasPoint {
    pub x: u8,
    pub y: u8,
}

impl CanvasPoint {
    #[must_use]
    pub const fn new(x: u8, y: u8) -> Option<Self> {
        if x < CANVAS_SIZE && y < CANVAS_SIZE {
            Some(Self { x, y })
        } else {
            None
        }
    }

    #[must_use]
    pub fn clamped(x: i32, y: i32) -> Self {
        let edge = i32::from(CANVAS_SIZE) - 1;
        let clamp = |value: i32| u8::try_from(value.clamp(0, edge)).unwrap_or_default();
        Self {
            x: clamp(x),
            y: clamp(y),
        }
    }

    const fn cell(self) -> usize {
        self.y as usize * CANVAS_SIZE as usize + self.x as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelGridError {
    WrongSize,
    BadPixel,
}

impl fmt::Display for PixelGridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongSize => write!(
                f,
                "A Kraddie layer is not {CANVAS_SIZE} by {CANVAS_SIZE} pixels"
            ),
            Self::BadPixel => f.write_str("A Kraddie layer has a pixel outside its palette"),
        }
    }
}

impl std::error::Error for PixelGridError {}

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Vec<String>", into = "Vec<String>")]
pub struct PixelGrid {
    cells: Vec<Option<PaletteIndex>>,
}

impl Default for PixelGrid {
    fn default() -> Self {
        Self {
            cells: vec![None; CELLS],
        }
    }
}

impl fmt::Debug for PixelGrid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(Vec::<String>::from(self.clone()))
            .finish()
    }
}

impl PixelGrid {
    #[must_use]
    pub fn get(&self, at: CanvasPoint) -> Option<PaletteIndex> {
        self.cells.get(at.cell()).copied().flatten()
    }

    pub fn set(&mut self, at: CanvasPoint, color: Option<PaletteIndex>) {
        if let Some(cell) = self.cells.get_mut(at.cell()) {
            *cell = color;
        }
    }

    pub fn set_signed(&mut self, x: i32, y: i32, color: Option<PaletteIndex>) {
        let point = u8::try_from(x)
            .ok()
            .zip(u8::try_from(y).ok())
            .and_then(|(x, y)| CanvasPoint::new(x, y));
        if let Some(point) = point {
            self.set(point, color);
        }
    }

    pub fn painted(&self) -> impl Iterator<Item = (CanvasPoint, PaletteIndex)> + '_ {
        self.cells.iter().enumerate().filter_map(|(cell, color)| {
            let color = (*color)?;
            let size = usize::from(CANVAS_SIZE);
            let x = u8::try_from(cell % size).ok()?;
            let y = u8::try_from(cell / size).ok()?;
            Some((CanvasPoint { x, y }, color))
        })
    }

    #[must_use]
    pub fn highest_index(&self) -> Option<PaletteIndex> {
        self.cells.iter().flatten().copied().max()
    }

    pub fn clear(&mut self) {
        self.cells.fill(None);
    }
}

fn mark(color: Option<PaletteIndex>) -> char {
    color
        .and_then(|index| char::from_digit(u32::from(index.0), INDEX_RADIX))
        .unwrap_or(EMPTY_MARK)
}

fn read_mark(mark: char) -> Result<Option<PaletteIndex>, PixelGridError> {
    if mark == EMPTY_MARK {
        return Ok(None);
    }
    let digit = mark.to_digit(INDEX_RADIX).ok_or(PixelGridError::BadPixel)?;
    let index = u8::try_from(digit).map_err(|_| PixelGridError::BadPixel)?;
    PaletteIndex::try_from(index).map(Some)
}

impl TryFrom<Vec<String>> for PixelGrid {
    type Error = PixelGridError;

    fn try_from(rows: Vec<String>) -> Result<Self, Self::Error> {
        let size = usize::from(CANVAS_SIZE);
        if rows.len() != size || rows.iter().any(|row| row.chars().count() != size) {
            return Err(PixelGridError::WrongSize);
        }
        let cells = rows
            .iter()
            .flat_map(|row| row.chars())
            .map(read_mark)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { cells })
    }
}

impl From<PixelGrid> for Vec<String> {
    fn from(grid: PixelGrid) -> Self {
        grid.cells
            .chunks(usize::from(CANVAS_SIZE))
            .map(|row| row.iter().copied().map(mark).collect())
            .collect()
    }
}
