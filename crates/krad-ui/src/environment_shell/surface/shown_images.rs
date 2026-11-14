use gpui_kit::{Context, Image, ImageFormat as PaintedFormat, ImageSource};
use krad_domain::{ChatImage, Mention, TerminalId};
use krad_protocol::{Answer, ChatCommand, Command};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::EnvironmentShell;
use super::{ChatPlace, ChatView};

#[derive(Clone, Debug)]
pub(in crate::environment_shell) enum Thumb {
    Asked,
    Shown(Arc<Image>),
    Unshown,
}

#[derive(Debug, Default)]
pub(in crate::environment_shell) enum ShownImages {
    #[default]
    Here,
    Fetched(HashMap<PathBuf, Thumb>),
}

impl ShownImages {
    pub(in crate::environment_shell) fn fetched() -> Self {
        Self::Fetched(HashMap::new())
    }

    pub(in crate::environment_shell) fn source(&self, path: &Path) -> Option<ImageSource> {
        match self {
            Self::Here => Some(ImageSource::from(path.to_path_buf())),
            Self::Fetched(thumbs) => match thumbs.get(path)? {
                Thumb::Shown(image) => Some(ImageSource::Image(Arc::clone(image))),
                Thumb::Asked | Thumb::Unshown => None,
            },
        }
    }

    pub(in crate::environment_shell) fn wanted<'a>(
        &mut self,
        mentions: impl IntoIterator<Item = &'a Mention>,
    ) -> Vec<ChatImage> {
        let Self::Fetched(thumbs) = self else {
            return Vec::new();
        };
        let mut wanted = Vec::new();
        for mention in mentions {
            let Mention::Image(image) = mention else {
                continue;
            };
            if thumbs.contains_key(&image.path) {
                continue;
            }
            thumbs.insert(image.path.clone(), Thumb::Asked);
            wanted.push(image.clone());
        }
        wanted
    }

    fn arrived(&mut self, path: PathBuf, image: Option<Arc<Image>>) {
        if let Self::Fetched(thumbs) = self {
            thumbs.insert(path, image.map_or(Thumb::Unshown, Thumb::Shown));
        }
    }
}

fn painted(answer: Answer) -> Option<Arc<Image>> {
    let Answer::ImageBytes { format, bytes } = answer else {
        return None;
    };
    let format = PaintedFormat::from_mime_type(format.media_type())?;
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

fn mentions_of(chat: &ChatView) -> impl Iterator<Item = &Mention> {
    chat.chat
        .messages
        .iter()
        .flat_map(|message| &message.mentions)
        .chain(&chat.attached)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn fetch_shown_images(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        if place.runner().is_none() {
            return;
        }
        let Some(chat) = self
            .live_mut(place)
            .and_then(|live| live.chats.get_mut(&id))
        else {
            return;
        };
        let mentions: Vec<Mention> = mentions_of(chat).cloned().collect();
        let wanted = chat.images.wanted(&mentions);
        for image in wanted {
            let path = image.path.clone();
            let read = Command::Chat(ChatCommand::ReadImage { image });
            let answer = self.ask_at(place, read, cx);
            let place = place.clone();
            cx.spawn(async move |this, cx| {
                let image = answer.await.ok().and_then(painted);
                this.update(cx, |shell, cx| {
                    if let Some(chat) = shell
                        .live_mut(&place)
                        .and_then(|live| live.chats.get_mut(&id))
                    {
                        chat.images.arrived(path, image);
                        cx.notify();
                    }
                })
                .unwrap_or_default();
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::ImageFormat;

    fn shot(path: &str) -> Mention {
        Mention::Image(ChatImage {
            path: PathBuf::from(path),
            format: ImageFormat::Png,
        })
    }

    #[test]
    fn a_hosted_chat_asks_for_each_image_once_and_paints_only_what_arrived() {
        let mut images = ShownImages::fetched();
        let mentions = [shot("/runner/1.png"), Mention::from(PathBuf::from("/a.rs"))];
        let wanted = images.wanted(&mentions);
        assert_eq!(wanted.len(), 1);
        assert_eq!(images.wanted(&mentions), Vec::<ChatImage>::new());
        assert!(images.source(Path::new("/runner/1.png")).is_none());
        let answer = Answer::ImageBytes {
            format: ImageFormat::Png,
            bytes: vec![0x89, b'P', b'N', b'G'],
        };
        images.arrived(PathBuf::from("/runner/1.png"), painted(answer));
        assert!(images.source(Path::new("/runner/1.png")).is_some());
        let refused = Answer::Unreadable {
            unreadable: krad_protocol::Unreadable::TooLarge { len: 9, limit: 1 },
        };
        images.arrived(PathBuf::from("/runner/2.png"), painted(refused));
        assert!(images.source(Path::new("/runner/2.png")).is_none());
    }

    #[test]
    fn a_local_chat_paints_its_images_from_their_paths() {
        let mut images = ShownImages::Here;
        assert_eq!(
            images.wanted(&[shot("/config/1.png")]),
            Vec::<ChatImage>::new()
        );
        assert!(images.source(Path::new("/config/1.png")).is_some());
    }
}
