mod animation;
mod color;
mod grid;
mod kraddie;
mod paint;
mod undo;

pub use animation::{animation_frame, KraddieAnimation, Pose, ANIMATION_FRAME};
pub use color::{KraddieColor, KraddieColorError};
pub use grid::{CanvasPoint, PaletteIndex, PixelGrid, PixelGridError, CANVAS_SIZE, MAX_PALETTE};
pub use kraddie::BodyStyle;
pub use paint::PaintTool;
pub use undo::{DesignHistory, MAX_UNDO};

use super::mood::KraddieMood;
use crate::attention::Switch;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_LAYERS: usize = 8;
const MAX_LAYER_NAME_CHARS: usize = 24;
const NEW_LAYER_NAME: &str = "Layer";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesignError {
    NoPalette,
    TooManyColors,
    NoLayers,
    TooManyLayers,
    ColorOutsidePalette,
    LayerNameTooLong,
}

impl fmt::Display for DesignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPalette => f.write_str("The design has no colors"),
            Self::TooManyColors => write!(f, "The design has more than {MAX_PALETTE} colors"),
            Self::NoLayers => f.write_str("The design has no layers"),
            Self::TooManyLayers => write!(f, "The design has more than {MAX_LAYERS} layers"),
            Self::ColorOutsidePalette => {
                f.write_str("The design paints with a color it does not have")
            }
            Self::LayerNameTooLong => write!(
                f,
                "A layer's name is longer than {MAX_LAYER_NAME_CHARS} characters"
            ),
        }
    }
}

impl std::error::Error for DesignError {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KraddieLayer {
    pub name: String,
    pub shown: Switch,
    pub pixels: PixelGrid,
}

impl KraddieLayer {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            shown: Switch::On,
            pixels: PixelGrid::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MoodLook {
    pub expression: PixelGrid,
    pub animation: KraddieAnimation,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MoodLooks {
    pub idle: MoodLook,
    pub working: MoodLook,
    pub waiting: MoodLook,
    pub done: MoodLook,
    pub error: MoodLook,
}

impl MoodLooks {
    #[must_use]
    pub const fn get(&self, mood: KraddieMood) -> &MoodLook {
        match mood {
            KraddieMood::Idle => &self.idle,
            KraddieMood::Working => &self.working,
            KraddieMood::Waiting => &self.waiting,
            KraddieMood::Done => &self.done,
            KraddieMood::Error => &self.error,
        }
    }

    pub const fn get_mut(&mut self, mood: KraddieMood) -> &mut MoodLook {
        match mood {
            KraddieMood::Idle => &mut self.idle,
            KraddieMood::Working => &mut self.working,
            KraddieMood::Waiting => &mut self.waiting,
            KraddieMood::Done => &mut self.done,
            KraddieMood::Error => &mut self.error,
        }
    }

    fn all(&self) -> impl Iterator<Item = &MoodLook> {
        KraddieMood::ALL.into_iter().map(|mood| self.get(mood))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaintTarget {
    Layer(usize),
    Expression(KraddieMood),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KraddieDesign {
    pub style: BodyStyle,
    pub palette: Vec<KraddieColor>,
    pub layers: Vec<KraddieLayer>,
    pub moods: MoodLooks,
}

impl KraddieDesign {
    /// # Errors
    /// Fails when the palette or the layers are empty or too many, a layer's name is too
    /// long, or a pixel names a color the palette does not have.
    pub fn validate(&self) -> Result<(), DesignError> {
        if self.palette.is_empty() {
            return Err(DesignError::NoPalette);
        }
        if self.palette.len() > MAX_PALETTE {
            return Err(DesignError::TooManyColors);
        }
        if self.layers.is_empty() {
            return Err(DesignError::NoLayers);
        }
        if self.layers.len() > MAX_LAYERS {
            return Err(DesignError::TooManyLayers);
        }
        if self
            .layers
            .iter()
            .any(|layer| layer.name.chars().count() > MAX_LAYER_NAME_CHARS)
        {
            return Err(DesignError::LayerNameTooLong);
        }
        let grids = self
            .layers
            .iter()
            .map(|layer| &layer.pixels)
            .chain(self.moods.all().map(|look| &look.expression));
        let outside = grids
            .filter_map(PixelGrid::highest_index)
            .any(|index| index.get() >= self.palette.len());
        if outside {
            return Err(DesignError::ColorOutsidePalette);
        }
        Ok(())
    }

    #[must_use]
    pub fn grid(&self, target: PaintTarget) -> Option<&PixelGrid> {
        match target {
            PaintTarget::Layer(index) => self.layers.get(index).map(|layer| &layer.pixels),
            PaintTarget::Expression(mood) => Some(&self.moods.get(mood).expression),
        }
    }

    pub fn grid_mut(&mut self, target: PaintTarget) -> Option<&mut PixelGrid> {
        match target {
            PaintTarget::Layer(index) => self.layers.get_mut(index).map(|layer| &mut layer.pixels),
            PaintTarget::Expression(mood) => Some(&mut self.moods.get_mut(mood).expression),
        }
    }

    pub fn paint(
        &mut self,
        target: PaintTarget,
        tool: PaintTool,
        from: CanvasPoint,
        to: CanvasPoint,
        color: PaletteIndex,
    ) -> bool {
        if color.get() >= self.palette.len() {
            return false;
        }
        let Some(grid) = self.grid_mut(target) else {
            return false;
        };
        tool.apply(grid, from, to, color);
        true
    }

    #[must_use]
    pub fn pixel(&self, at: CanvasPoint, mood: KraddieMood) -> Option<KraddieColor> {
        let face = self.moods.get(mood).expression.get(at);
        let body = self
            .layers
            .iter()
            .rev()
            .filter(|layer| layer.shown.is_on())
            .find_map(|layer| layer.pixels.get(at));
        face.or(body)
            .and_then(|index| self.palette.get(index.get()).copied())
    }

    pub fn add_layer(&mut self) -> Option<usize> {
        if self.layers.len() >= MAX_LAYERS {
            return None;
        }
        let number = self.layers.len() + 1;
        self.layers
            .push(KraddieLayer::new(&format!("{NEW_LAYER_NAME} {number}")));
        Some(self.layers.len() - 1)
    }

    pub fn remove_layer(&mut self, index: usize) -> bool {
        if self.layers.len() <= 1 || index >= self.layers.len() {
            return false;
        }
        self.layers.remove(index);
        true
    }

    pub fn toggle_layer(&mut self, index: usize) -> bool {
        let Some(layer) = self.layers.get_mut(index) else {
            return false;
        };
        layer.shown = layer.shown.toggled();
        true
    }

    pub fn add_color(&mut self, color: KraddieColor) -> Option<PaletteIndex> {
        if self.palette.len() >= MAX_PALETTE {
            return None;
        }
        self.palette.push(color);
        PaletteIndex::try_from(self.palette.len() - 1).ok()
    }

    pub fn set_color(&mut self, index: PaletteIndex, color: KraddieColor) -> bool {
        let Some(slot) = self.palette.get_mut(index.get()) else {
            return false;
        };
        *slot = color;
        true
    }
}

#[cfg(test)]
mod tests;
