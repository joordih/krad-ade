use gpui_kit::{AppContext as _, Context};
use krad_domain::{ChatImage, EnvironmentName, ImageFormat, Mention, TerminalId, ToastKind};
use krad_protocol::{AgentRef, ChatCommand, Command, PastedImage};

use super::super::super::EnvironmentShell;

pub(super) fn chat_ref(env: &EnvironmentName, id: TerminalId) -> AgentRef {
    AgentRef {
        environment: env.clone(),
        terminal: id,
    }
}

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
            &chat.environment,
            chat.terminal,
            vec![Mention::Image(image)],
            cx,
        );
    }

    pub(super) fn attach_mentions(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        mentions: Vec<Mention>,
        cx: &mut Context<Self>,
    ) {
        let (paths, images) = split_images(mentions);
        self.attach(env, id, paths, cx);
        for image in images {
            Self::upload_image_file(env, id, image, cx);
        }
    }

    fn upload_image_file(
        env: &EnvironmentName,
        id: TerminalId,
        image: ChatImage,
        cx: &mut Context<Self>,
    ) {
        let read = cx.background_spawn(async move { std::fs::read(&image.path) });
        let chat = chat_ref(env, id);
        let format = image.format;
        cx.spawn(async move |this, cx| {
            let read = read.await;
            this.update(cx, |shell, _| match read {
                Ok(bytes) => shell.upload_image(chat, format, bytes),
                Err(error) => shell.push_toast(ToastKind::Error, error.to_string()),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn upload_image(&self, chat: AgentRef, format: ImageFormat, bytes: Vec<u8>) {
        let image = PastedImage { format, bytes };
        self.runtime
            .client
            .post(Command::Chat(ChatCommand::AttachImage { chat, image }));
    }

    pub(in crate::environment_shell) fn forget_chat_images(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) {
        let chat = chat_ref(env, id);
        self.runtime
            .client
            .post(Command::Chat(ChatCommand::ForgetImages { chat }));
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
