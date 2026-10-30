use krad_domain::ChatImage;
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::wire::Message;
use crate::mention::{read_image, shown_path, EncodedImage};

const UNREADABLE_LEAD: &str = "[The image ";
const UNREADABLE_TAIL: &str = " could not be read]";

static NO_IMAGES: Attachments = Attachments(BTreeMap::new());

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Attached {
    Image(EncodedImage),
    Unreadable(String),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Attachments(BTreeMap<PathBuf, Attached>);

fn attached(image: &ChatImage) -> Attached {
    read_image(image).map_or_else(
        |_| {
            Attached::Unreadable(format!(
                "{UNREADABLE_LEAD}{}{UNREADABLE_TAIL}",
                shown_path(&image.path)
            ))
        },
        Attached::Image,
    )
}

impl Attachments {
    pub(crate) fn none() -> &'static Self {
        &NO_IMAGES
    }

    pub(crate) fn read(messages: &[Message]) -> Self {
        let mut read = BTreeMap::new();
        for image in messages.iter().flat_map(Message::images) {
            read.entry(image.path.clone())
                .or_insert_with(|| attached(image));
        }
        Self(read)
    }

    pub(crate) fn of<'a>(&'a self, images: &'a [ChatImage]) -> impl Iterator<Item = &'a Attached> {
        images.iter().filter_map(|image| self.0.get(&image.path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::ImageFormat;

    #[test]
    fn each_image_is_read_once_and_a_missing_one_becomes_a_note() {
        let dir =
            std::env::temp_dir().join(format!("krad-engine-attachments-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap_or_default();
        let shot = dir.join("1.png");
        std::fs::write(&shot, b"png").unwrap_or_default();
        let image = |path: PathBuf| ChatImage {
            path,
            format: ImageFormat::Png,
        };
        let present = image(shot);
        let missing = image(PathBuf::from("/nowhere/gone.png"));
        let messages = [
            Message::User {
                text: "look".to_owned(),
                images: vec![present.clone(), missing.clone()],
            },
            Message::User {
                text: "again".to_owned(),
                images: vec![present.clone()],
            },
        ];
        let read = Attachments::read(&messages);
        let images = [present, missing];
        let found: Vec<&Attached> = read.of(&images).collect();
        assert!(matches!(found[0], Attached::Image(encoded) if encoded.data == "cG5n"));
        assert_eq!(
            found[1],
            &Attached::Unreadable("[The image /nowhere/gone.png could not be read]".to_owned())
        );
        assert_eq!(Attachments::none().of(&images).count(), 0);
    }
}
