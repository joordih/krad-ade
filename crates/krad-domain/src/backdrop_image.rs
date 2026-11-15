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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Backdrops {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<BackdropImage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<BackdropImage>,
}

impl Backdrops {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.window.is_none() && self.chat.is_none()
    }

    #[must_use]
    pub const fn get(&self, place: BackdropPlace) -> Option<&BackdropImage> {
        match place {
            BackdropPlace::Window => self.window.as_ref(),
            BackdropPlace::Chat => self.chat.as_ref(),
        }
    }

    pub const fn get_mut(&mut self, place: BackdropPlace) -> &mut Option<BackdropImage> {
        match place {
            BackdropPlace::Window => &mut self.window,
            BackdropPlace::Chat => &mut self.chat,
        }
    }

    pub fn adopt_from(&mut self, settings: &mut Settings) {
        let window = settings.backdrop_image.take();
        let chat = settings.chat_backdrop_image.take();
        if self.is_empty() {
            self.window = window;
            self.chat = chat;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(as = "u8"))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
        let mut backdrops = Backdrops::default();
        assert_eq!(serde_json::to_string(&backdrops)?, "{}");
        *backdrops.get_mut(BackdropPlace::Chat) = Some(BackdropImage::new("chat.png"));
        assert_eq!(backdrops.get(BackdropPlace::Window), None);
        assert_eq!(
            backdrops
                .get(BackdropPlace::Chat)
                .map(|image| image.file.clone()),
            Some(PathBuf::from("chat.png"))
        );
        let back: Backdrops = serde_json::from_str(&serde_json::to_string(&backdrops)?)?;
        assert_eq!(back, backdrops);
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
