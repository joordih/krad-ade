mod state;
pub use state::*;

use crate::ids::{TabId, TerminalId};
use serde::{Deserialize, Serialize};
pub const MAIN_RATIO_MIN: f32 = 0.2;
pub const MAIN_RATIO_MAX: f32 = 0.8;
pub const MAIN_RATIO_DEFAULT: f32 = 0.6;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Arrangement {
    #[default]
    Columns,
    Main,
    Grid,
    Max,
}

impl Arrangement {
    pub const ALL: [Self; 4] = [Self::Columns, Self::Main, Self::Grid, Self::Max];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Columns => "Columns",
            Self::Main => "Main",
            Self::Grid => "Grid",
            Self::Max => "Max",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Columns => Self::Main,
            Self::Main => Self::Grid,
            Self::Grid => Self::Max,
            Self::Max => Self::Columns,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Tile {
    pub id: TerminalId,
    pub share: f32,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MosaicTab {
    pub id: TabId,
    pub name: String,
    pub arrangement: Arrangement,
    pub tiles: Vec<Tile>,
    pub main_ratio: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct UnitRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DividerKind {
    Column(usize),
    Main,
    Stack(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DividerAxis {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Divider {
    pub kind: DividerKind,
    pub axis: DividerAxis,
    pub at: f32,
    pub from: f32,
    pub len: f32,
}

impl MosaicTab {
    fn new(id: TabId, name: String) -> Self {
        Self {
            id,
            name,
            arrangement: Arrangement::default(),
            tiles: Vec::new(),
            main_ratio: MAIN_RATIO_DEFAULT,
        }
    }

    fn contains(&self, id: TerminalId) -> bool {
        self.tiles.iter().any(|tile| tile.id == id)
    }

    fn insert(&mut self, id: TerminalId) {
        self.insert_at(self.tiles.len(), id);
    }

    fn insert_at(&mut self, index: usize, id: TerminalId) {
        let count = self.tiles.len();
        let fair = 1.0 / count_f32(count + 1);
        for tile in &mut self.tiles {
            tile.share *= 1.0 - fair;
        }
        self.tiles
            .insert(index.min(count), Tile { id, share: fair });
        normalize(&mut self.tiles);
    }

    fn remove(&mut self, id: TerminalId) -> bool {
        let before = self.tiles.len();
        self.tiles.retain(|tile| tile.id != id);
        normalize(&mut self.tiles);
        self.tiles.len() != before
    }

    #[must_use]
    pub fn dividers(&self) -> Vec<Divider> {
        match self.arrangement {
            Arrangement::Columns => boundaries(&self.tiles)
                .into_iter()
                .enumerate()
                .map(|(index, at)| Divider {
                    kind: DividerKind::Column(index),
                    axis: DividerAxis::Vertical,
                    at,
                    from: 0.0,
                    len: 1.0,
                })
                .collect(),
            Arrangement::Main => {
                let Some((_, stack)) = self.tiles.split_first() else {
                    return Vec::new();
                };
                if stack.is_empty() {
                    return Vec::new();
                }
                let ratio = self.main_ratio;
                let main = Divider {
                    kind: DividerKind::Main,
                    axis: DividerAxis::Vertical,
                    at: ratio,
                    from: 0.0,
                    len: 1.0,
                };
                std::iter::once(main)
                    .chain(
                        boundaries(stack)
                            .into_iter()
                            .enumerate()
                            .map(|(index, at)| Divider {
                                kind: DividerKind::Stack(index),
                                axis: DividerAxis::Horizontal,
                                at,
                                from: ratio,
                                len: 1.0 - ratio,
                            }),
                    )
                    .collect()
            }
            Arrangement::Grid | Arrangement::Max => Vec::new(),
        }
    }
}

pub(crate) fn count_f32(count: usize) -> f32 {
    u16::try_from(count).map_or(f32::from(u16::MAX), f32::from)
}

fn normalize(tiles: &mut [Tile]) {
    let total: f32 = tiles.iter().map(|tile| tile.share).sum();
    if total > 0.0 {
        for tile in tiles {
            tile.share /= total;
        }
    }
}

fn spans(tiles: &[Tile]) -> Vec<(TerminalId, f32, f32)> {
    let total: f32 = tiles.iter().map(|tile| tile.share).sum();
    let mut start = 0.0;
    tiles
        .iter()
        .map(|tile| {
            let len = if total > 0.0 {
                tile.share / total
            } else {
                1.0 / count_f32(tiles.len())
            };
            let span = (tile.id, start, len);
            start += len;
            span
        })
        .collect()
}

fn boundaries(tiles: &[Tile]) -> Vec<f32> {
    let spans = spans(tiles);
    spans
        .iter()
        .take(spans.len().saturating_sub(1))
        .map(|(_, start, len)| start + len)
        .collect()
}
