use gpui_kit::{AppContext as _, Context};
use krad_domain::{ChatImage, EnvironmentName, ImageFormat, Mention, TerminalId, ToastKind};
use krad_protocol::{AgentRef, ChatCommand, PastedImage};

use super::super::super::EnvironmentShell;
use super::super::ChatPlace;

fn split_images(mentions: Vec<Mention>) -> (Vec<Mention>, Vec<ChatImage>) {
    let mut paths = Vec::new();
    let mut images = Vec::new();
    for mention in mentions {
        match mention {
            Mention::Image(image) => images.push(image),
            Mention::Path(_) => paths.push(mention),
        }
    }
    (paths, images)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn image_attached(
        &mut self,
        chat: &AgentRef,
        image: ChatImage,
        cx: &mut Context<Self>,
    ) {
        self.attach(
            &ChatPlace::here(&chat.environment),
            chat.terminal,
            vec![Mention::Image(image)],
            cx,
        );
    }

    pub(super) fn attach_mentions(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        mentions: Vec<Mention>,
        cx: &mut Context<Self>,
    ) {
        let (paths, images) = split_images(mentions);
        self.attach(place, id, paths, cx);
        for image in images {
            Self::upload_image_file(place, id, image, cx);
        }
    }

    fn upload_image_file(
        place: &ChatPlace,
        id: TerminalId,
        image: ChatImage,
        cx: &mut Context<Self>,
    ) {
        let read = cx.background_spawn(async move { std::fs::read(&image.path) });
        let place = place.clone();
        let format = image.format;
        cx.spawn(async move |this, cx| {
            let read = read.await;
            this.update(cx, |shell, _| match read {
                Ok(bytes) => shell.upload_image(&place, id, (format, bytes)),
                Err(error) => shell.push_toast(ToastKind::Error, error.to_string()),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn upload_image(
        &self,
        place: &ChatPlace,
        id: TerminalId,
        (format, bytes): (ImageFormat, Vec<u8>),
    ) {
        let image = PastedImage { format, bytes };
        let chat = place.chat(id);
        self.post_chat(place, ChatCommand::AttachImage { chat, image });
    }

    pub(in crate::environment_shell) fn forget_chat_images(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) {
        let place = ChatPlace::here(env);
        self.post_chat(
            &place,
            ChatCommand::ForgetImages {
                chat: place.chat(id),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn picked_images_go_to_the_runtime_and_other_paths_stay_mentions() {
        let shot = PathBuf::from("/Users/me/shot.png");
        let file = Mention::from(PathBuf::from("/repo/a.rs"));
        let (paths, images) = split_images(vec![Mention::from(shot.clone()), file.clone()]);
        assert_eq!(paths, [file]);
        assert_eq!(
            images,
            [ChatImage {
                path: shot,
                format: ImageFormat::Png
            }]
        );
    }
}
