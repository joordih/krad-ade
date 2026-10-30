use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

const PNG_EXTENSION: &str = "png";
const JPEG_EXTENSIONS: [&str; 2] = ["jpg", "jpeg"];
const GIF_EXTENSION: &str = "gif";
const WEBP_EXTENSION: &str = "webp";
const PNG_MEDIA_TYPE: &str = "image/png";
const JPEG_MEDIA_TYPE: &str = "image/jpeg";
const GIF_MEDIA_TYPE: &str = "image/gif";
const WEBP_MEDIA_TYPE: &str = "image/webp";
const MENTIONS_SEPARATOR: &str = "\n\n";
const PATH_SEPARATOR: &str = "\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl ImageFormat {
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Png => PNG_MEDIA_TYPE,
            Self::Jpeg => JPEG_MEDIA_TYPE,
            Self::Gif => GIF_MEDIA_TYPE,
            Self::Webp => WEBP_MEDIA_TYPE,
        }
    }

    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => PNG_EXTENSION,
            Self::Jpeg => JPEG_EXTENSIONS[0],
            Self::Gif => GIF_EXTENSION,
            Self::Webp => WEBP_EXTENSION,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownImageFormat;

impl fmt::Display for UnknownImageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not an image format a chat can send")
    }
}

impl std::error::Error for UnknownImageFormat {}

impl FromStr for ImageFormat {
    type Err = UnknownImageFormat;

    fn from_str(extension: &str) -> Result<Self, Self::Err> {
        let extension = extension.to_ascii_lowercase();
        match extension.as_str() {
            PNG_EXTENSION => Ok(Self::Png),
            GIF_EXTENSION => Ok(Self::Gif),
            WEBP_EXTENSION => Ok(Self::Webp),
            jpeg if JPEG_EXTENSIONS.contains(&jpeg) => Ok(Self::Jpeg),
            _ => Err(UnknownImageFormat),
        }
    }
}

impl TryFrom<&Path> for ImageFormat {
    type Error = UnknownImageFormat;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        path.extension()
            .and_then(|extension| extension.to_str())
            .ok_or(UnknownImageFormat)?
            .parse()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatImage {
    pub path: PathBuf,
    pub format: ImageFormat,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Mention {
    Path(PathBuf),
    Image(ChatImage),
}

impl Mention {
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::Path(path) | Self::Image(ChatImage { path, .. }) => path,
        }
    }
}

impl From<PathBuf> for Mention {
    fn from(path: PathBuf) -> Self {
        match ImageFormat::try_from(path.as_path()) {
            Ok(format) => Self::Image(ChatImage { path, format }),
            Err(UnknownImageFormat) => Self::Path(path),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Prompt {
    pub text: String,
    pub mentions: Vec<Mention>,
}

impl Prompt {
    #[must_use]
    pub fn new(text: &str, mentions: Vec<Mention>) -> Self {
        Self {
            text: text.trim().to_owned(),
            mentions,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.mentions.is_empty()
    }

    pub fn images(&self) -> impl Iterator<Item = &ChatImage> {
        self.mentions.iter().filter_map(|mention| match mention {
            Mention::Image(image) => Some(image),
            Mention::Path(_) => None,
        })
    }

    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.mentions.iter().filter_map(|mention| match mention {
            Mention::Path(path) => Some(path.as_path()),
            Mention::Image(_) => None,
        })
    }

    #[must_use]
    pub fn text_with_paths(&self, shown: impl Fn(&Path) -> String) -> String {
        let paths: Vec<String> = self.paths().map(shown).collect();
        if paths.is_empty() {
            return self.text.clone();
        }
        let listed = paths.join(PATH_SEPARATOR);
        if self.text.is_empty() {
            return listed;
        }
        format!("{}{MENTIONS_SEPARATOR}{listed}", self.text)
    }
}

impl From<String> for Prompt {
    fn from(text: String) -> Self {
        Self {
            text,
            mentions: Vec::new(),
        }
    }
}

impl From<&str> for Prompt {
    fn from(text: &str) -> Self {
        Self::from(text.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(path: &Path) -> String {
        path.display().to_string()
    }

    #[test]
    fn a_picked_path_is_an_image_only_when_its_extension_says_so() {
        assert_eq!(
            Mention::from(PathBuf::from("/shots/Screen.PNG")),
            Mention::Image(ChatImage {
                path: PathBuf::from("/shots/Screen.PNG"),
                format: ImageFormat::Png,
            })
        );
        assert!(matches!(
            Mention::from(PathBuf::from("/shots/a.jpeg")),
            Mention::Image(ChatImage {
                format: ImageFormat::Jpeg,
                ..
            })
        ));
        assert_eq!(
            Mention::from(PathBuf::from("/src/main.rs")),
            Mention::Path(PathBuf::from("/src/main.rs"))
        );
        assert_eq!(
            Mention::from(PathBuf::from("/src")),
            Mention::Path(PathBuf::from("/src"))
        );
        assert_eq!("svg".parse::<ImageFormat>(), Err(UnknownImageFormat));
        assert_eq!(ImageFormat::Webp.media_type(), "image/webp");
    }

    #[test]
    fn files_go_by_their_path_after_the_text_and_images_apart() {
        let prompt = Prompt::new(
            "  Why does this fail?\n",
            vec![
                Mention::from(PathBuf::from("/repo/src/lib.rs")),
                Mention::from(PathBuf::from("/shots/1.png")),
                Mention::from(PathBuf::from("/repo/docs")),
            ],
        );
        assert_eq!(
            prompt.text_with_paths(shown),
            "Why does this fail?\n\n/repo/src/lib.rs\n/repo/docs"
        );
        assert_eq!(
            prompt
                .images()
                .map(|image| image.path.as_path())
                .collect::<Vec<_>>(),
            [Path::new("/shots/1.png")]
        );
        let only_files = Prompt::new("", vec![Mention::from(PathBuf::from("/a.txt"))]);
        assert_eq!(only_files.text_with_paths(shown), "/a.txt");
        assert!(!only_files.is_empty());
        assert!(Prompt::new(" ", Vec::new()).is_empty());
        assert_eq!(Prompt::from("hi").text_with_paths(shown), "hi");
    }
}
