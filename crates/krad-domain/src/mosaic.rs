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
}

fn count_f32(count: usize) -> f32 {
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
