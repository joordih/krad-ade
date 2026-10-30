use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use krad_domain::{wsl_linux_path, ChatImage, Prompt};
use std::io;
use std::path::Path;

use crate::process::Runner;

const DATA_URL_SCHEME: &str = "data:";
const DATA_URL_BASE64: &str = ";base64,";
const LINE_BREAK: &str = "\n";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EncodedImage {
    pub(crate) media_type: &'static str,
    pub(crate) data: String,
}

impl EncodedImage {
    pub(crate) fn new(image: &ChatImage, bytes: &[u8]) -> Self {
        Self {
            media_type: image.format.media_type(),
            data: STANDARD.encode(bytes),
        }
    }

    pub(crate) fn data_url(&self) -> String {
        format!(
            "{DATA_URL_SCHEME}{}{DATA_URL_BASE64}{}",
            self.media_type, self.data
        )
    }
}

pub(crate) fn read_image(image: &ChatImage) -> io::Result<EncodedImage> {
    std::fs::read(&image.path).map(|bytes| EncodedImage::new(image, &bytes))
}

pub(crate) fn shown_path(path: &Path) -> String {
    path.display().to_string()
}

pub(crate) fn served_path(runner: &Runner, path: &Path) -> String {
    match runner {
        Runner::Wsl { distro, .. } => {
            wsl_linux_path(distro, path).unwrap_or_else(|| shown_path(path))
        }
        Runner::Native(_) => shown_path(path),
    }
}

pub(crate) struct ReadPrompt {
    pub(crate) text: String,
    pub(crate) images: Vec<EncodedImage>,
}

pub(crate) fn read_prompt(prompt: &Prompt, shown: impl Fn(&Path) -> String) -> ReadPrompt {
    let mut unreadable = Vec::new();
    let images = prompt
        .images()
        .filter_map(|image| {
            let encoded = read_image(image).ok();
            if encoded.is_none() {
                unreadable.push(shown(&image.path));
            }
            encoded
        })
        .collect();
    let text = prompt.text_with_paths(&shown);
    let text = std::iter::once(text)
        .chain(unreadable)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(LINE_BREAK);
    ReadPrompt { text, images }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{ImageFormat, Mention};
    use std::path::PathBuf;

    #[test]
    fn an_image_reads_as_base64_and_an_unreadable_one_goes_by_its_path() {
        let dir = std::env::temp_dir().join("krad-chat-mention-read");
        std::fs::create_dir_all(&dir).unwrap_or_else(|_| unreachable!("the fixture is valid"));
        let shot = dir.join("1.png");
        std::fs::write(&shot, b"png").unwrap_or_else(|_| unreachable!("the fixture is valid"));
        let prompt = Prompt::new(
            "Look",
            vec![
                Mention::from(shot.clone()),
                Mention::from(PathBuf::from("/nowhere/gone.gif")),
            ],
        );
        let read = read_prompt(&prompt, shown_path);
        assert_eq!(
            read.images,
            [EncodedImage {
                media_type: "image/png",
                data: "cG5n".to_owned(),
            }]
        );
        assert_eq!(read.text, "Look\n/nowhere/gone.gif");
        let image = ChatImage {
            path: shot,
            format: ImageFormat::Png,
        };
        assert_eq!(
            EncodedImage::new(&image, b"png").data_url(),
            "data:image/png;base64,cG5n"
        );
    }
}
