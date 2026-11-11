use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::settings::Settings;

const DEFAULT_OPACITY: BackdropPercent = BackdropPercent::new(80);
const DEFAULT_BLUR: BackdropPercent = BackdropPercent::new(0);
const DEFAULT_DIM: BackdropPercent = BackdropPercent::new(35);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BackdropPlace {
    Window,
    Chat,
}

impl BackdropPlace {
    pub const ALL: [Self; 2] = [Self::Window, Self::Chat];
}

impl Settings {
    #[must_use]
    pub const fn backdrop(&self, place: BackdropPlace) -> Option<&BackdropImage> {
        match place {
            BackdropPlace::Window => self.backdrop_image.as_ref(),
            BackdropPlace::Chat => self.chat_backdrop_image.as_ref(),
        }
    }

    pub const fn backdrop_mut(&mut self, place: BackdropPlace) -> &mut Option<BackdropImage> {
        match place {
            BackdropPlace::Window => &mut self.backdrop_image,
            BackdropPlace::Chat => &mut self.chat_backdrop_image,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackdropFit {
    #[default]
    Fill,
    Fit,
    Tile,
    Center,
}

impl BackdropFit {
    pub const ALL: [Self; 4] = [Self::Fill, Self::Fit, Self::Tile, Self::Center];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Fill => "Fill",
            Self::Fit => "Fit",
            Self::Tile => "Tile",
            Self::Center => "Center",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(from = "u8", into = "u8")]
pub struct BackdropPercent(u8);

impl BackdropPercent {
    pub const MAX: u8 = 100;

    #[must_use]
    pub const fn new(percent: u8) -> Self {
        if percent > Self::MAX {
            Self(Self::MAX)
        } else {
            Self(percent)
        }
    }

    #[must_use]
    pub const fn percent(self) -> u8 {
        self.0
    }

    #[must_use]
    pub fn fraction(self) -> f32 {
        f32::from(self.0) / f32::from(Self::MAX)
    }
}

impl From<u8> for BackdropPercent {
    fn from(percent: u8) -> Self {
        Self::new(percent)
    }
}

impl From<BackdropPercent> for u8 {
    fn from(percent: BackdropPercent) -> Self {
        percent.0
    }
}

const fn default_opacity() -> BackdropPercent {
    DEFAULT_OPACITY
}

const fn default_blur() -> BackdropPercent {
    DEFAULT_BLUR
}

const fn default_dim() -> BackdropPercent {
    DEFAULT_DIM
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackdropImage {
    pub file: PathBuf,
    #[serde(default)]
    pub fit: BackdropFit,
    #[serde(default = "default_opacity")]
    pub opacity: BackdropPercent,
    #[serde(default = "default_blur")]
    pub blur: BackdropPercent,
    #[serde(default = "default_dim")]
    pub dim: BackdropPercent,
}

impl BackdropImage {
    #[must_use]
    pub fn new(file: impl AsRef<Path>) -> Self {
        Self {
            file: file.as_ref().to_path_buf(),
            fit: BackdropFit::default(),
            opacity: DEFAULT_OPACITY,
            blur: DEFAULT_BLUR,
            dim: DEFAULT_DIM,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backdrop_image_round_trips_and_missing_fields_take_their_defaults(
    ) -> serde_json::Result<()> {
        let image = BackdropImage {
            fit: BackdropFit::Tile,
            opacity: BackdropPercent::new(60),
            blur: BackdropPercent::new(20),
            dim: BackdropPercent::new(50),
            ..BackdropImage::new("backdrop/sky.png")
        };
        let text = serde_json::to_string(&image)?;
        assert_eq!(
            text,
            r#"{"file":"backdrop/sky.png","fit":"tile","opacity":60,"blur":20,"dim":50}"#
        );
        assert_eq!(serde_json::from_str::<BackdropImage>(&text)?, image);
        let bare: BackdropImage = serde_json::from_str(r#"{"file":"sky.png"}"#)?;
        assert_eq!(bare, BackdropImage::new("sky.png"));
        Ok(())
    }

    #[test]
    fn the_window_and_the_chat_keep_their_own_image() -> serde_json::Result<()> {
        let mut settings = Settings::default();
        *settings.backdrop_mut(BackdropPlace::Chat) = Some(BackdropImage::new("chat.png"));
        assert_eq!(settings.backdrop(BackdropPlace::Window), None);
        assert_eq!(
            settings
                .backdrop(BackdropPlace::Chat)
                .map(|image| image.file.clone()),
            Some(PathBuf::from("chat.png"))
        );
        let back: Settings = serde_json::from_str(&serde_json::to_string(&settings)?)?;
        assert_eq!(back.chat_backdrop_image, settings.chat_backdrop_image);
        assert_eq!(back.backdrop_image, None);
        Ok(())
    }

    #[test]
    fn a_percent_past_the_whole_is_kept_at_the_whole() -> serde_json::Result<()> {
        let read: BackdropPercent = serde_json::from_str("250")?;
        assert_eq!(read.percent(), BackdropPercent::MAX);
        assert!((BackdropPercent::new(50).fraction() - 0.5).abs() < f32::EPSILON);
        Ok(())
    }
}
