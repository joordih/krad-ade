use gpui_kit::{
    div, img, px, rgb, AnyElement, AppContext as _, ClipboardEntry, ClipboardItem, Context,
    ElementId, InteractiveElement, IntoElement, ObjectFit, ParentElement, PathPromptOptions,
    SharedString, StatefulInteractiveElement, Styled, StyledImage,
};
use krad_domain::{EnvironmentName, ImageFormat, Mention, TerminalId};
use krad_store::{chat_images_dir, delete_chat_images, save_pasted_image};
use std::path::{Path, PathBuf};

use super::super::EnvironmentShell;
use super::SurfaceChat;
use crate::components::{icon, tooltip_view, IconName};
use crate::theme::{
    palette, Palette, ICON_SM, ICON_XS, LINE_XS, RADIUS_MD, RADIUS_SM, SPACE_1, SPACE_1_5, SPACE_2,
    TEXT_XS,
};

pub(super) const PASTE_KEY: &str = "v";
const PICK_PROMPT: &str = "Attach";
const TOOLTIP_ATTACH: &str = "Attach files, folders or images";
const TOOLTIP_REMOVE: &str = "Remove";
const ATTACH_PREFIX: &str = "surface-attach";
const REMOVE_PREFIX: &str = "surface-mention-remove";
const ATTACH_SIZE: f32 = 24.0;
const THUMB_SIZE: f32 = 48.0;
const SENT_THUMB_SIZE: f32 = 96.0;
const CHIP_H: f32 = 24.0;
const CHIP_MAX_W: f32 = 220.0;
const REMOVE_SIZE: f32 = 16.0;
const REMOVE_INSET: f32 = -4.0;

fn mention_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn clipboard_image(item: &ClipboardItem) -> Option<(ImageFormat, Vec<u8>)> {
    let entries = item.entries();
    let has_text = entries
        .iter()
        .any(|entry| matches!(entry, ClipboardEntry::String(_)));
    if has_text {
        return None;
    }
    entries.iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) => image
            .format
            .extension()
            .parse()
            .ok()
            .map(|format| (format, image.bytes.clone())),
        ClipboardEntry::String(_) | ClipboardEntry::ExternalPaths(_) => None,
    })
}

fn added(attached: &mut Vec<Mention>, mentions: Vec<Mention>) {
    for mention in mentions {
        if !attached.contains(&mention) {
            attached.push(mention);
        }
    }
}

fn thumbnail(path: &Path, size: f32) -> gpui_kit::Img {
    img(path.to_path_buf())
        .size(px(size))
        .rounded(px(RADIUS_MD))
        .object_fit(ObjectFit::Cover)
}

fn file_chip(path: &Path, pal: &Palette) -> gpui_kit::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1))
        .h(px(CHIP_H))
        .max_w(px(CHIP_MAX_W))
        .px(px(SPACE_2))
        .rounded(px(RADIUS_SM))
        .bg(rgb(pal.bg_panel))
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_default))
        .child(icon(IconName::File, ICON_XS, pal.text_muted))
        .child(div().truncate().child(mention_name(path)))
}

fn mention_element(mention: &Mention, size: f32, pal: &Palette) -> AnyElement {
    match mention {
        Mention::Image(image) => thumbnail(&image.path, size).into_any_element(),
        Mention::Path(path) => file_chip(path, pal).into_any_element(),
    }
}

pub(super) fn sent_mentions(mentions: &[Mention], pal: &Palette) -> Option<AnyElement> {
    if mentions.is_empty() {
        return None;
    }
    Some(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .justify_end()
            .items_end()
            .gap(px(SPACE_1_5))
            .children(
                mentions
                    .iter()
                    .map(|mention| mention_element(mention, SENT_THUMB_SIZE, pal)),
            )
            .into_any_element(),
    )
}

impl EnvironmentShell {
    fn attach(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        mentions: Vec<Mention>,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
            .filter(|chat| chat.structured().is_some())
        else {
            return;
        };
        added(&mut chat.attached, mentions);
        cx.notify();
    }

    pub(super) fn attach_paths(
        &mut self,
        id: TerminalId,
        paths: &[PathBuf],
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let mentions = paths.iter().cloned().map(Mention::from).collect();
        self.attach(&env, id, mentions, cx);
    }

    fn pick_mentions(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some(SharedString::from(PICK_PROMPT)),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = answer.await else {
                return;
            };
            let mentions = paths.into_iter().map(Mention::from).collect();
            this.update(cx, |shell, cx| shell.attach(&env, id, mentions, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn paste_chat_image(&mut self, id: TerminalId, cx: &mut Context<Self>) -> bool {
        let env = self.active.clone();
        let structured = self
            .envs
            .get(&env)
            .and_then(|live| live.chats.get(&id))
            .is_some_and(|chat| chat.structured().is_some());
        if !structured {
            return false;
        }
        let Some((format, bytes)) = cx
            .read_from_clipboard()
            .and_then(|item| clipboard_image(&item))
        else {
            return false;
        };
        let owner = env.clone();
        let save =
            cx.background_spawn(async move { save_pasted_image(&owner, id, format, &bytes) });
        cx.spawn(async move |this, cx| {
            let saved = save.await;
            this.update(cx, |shell, cx| match saved {
                Ok(image) => shell.attach(&env, id, vec![Mention::Image(image)], cx),
                Err(error) => shell.push_toast(error.to_string()),
            })
            .unwrap_or_default();
        })
        .detach();
        true
    }

    fn remove_mention(&mut self, id: TerminalId, index: usize, cx: &mut Context<Self>) {
        let env = self.active.clone();
        if let Some(chat) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
            .filter(|chat| index < chat.attached.len())
        {
            chat.attached.remove(index);
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn forget_chat_images(
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let Some(dir) = chat_images_dir(env, id) else {
            return;
        };
        let delete = cx.background_spawn(async move { delete_chat_images(&dir) });
        cx.spawn(async move |this, cx| {
            if let Err(error) = delete.await {
                this.update(cx, |shell, _| shell.push_toast(error.to_string()))
                    .unwrap_or_default();
            }
        })
        .detach();
    }

    pub(super) fn attach_control(id: TerminalId, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .id((ATTACH_PREFIX, id.as_u64()))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(ATTACH_SIZE))
            .rounded(px(RADIUS_MD))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .tooltip(|_, cx| tooltip_view(TOOLTIP_ATTACH.into(), cx))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.pick_mentions(id, cx);
            }))
            .child(icon(IconName::Attach, ICON_SM, pal.text_muted))
            .into_any_element()
    }

    pub(super) fn attached_tray(
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if chat.attached.is_empty() {
            return None;
        }
        let pal = palette(cx);
        let chips = chat.attached.iter().enumerate().map(|(index, mention)| {
            let raw = u64::try_from(index).unwrap_or(u64::MAX);
            let remove = div()
                .id(ElementId::Name(
                    format!("{REMOVE_PREFIX}-{}-{raw}", id.as_u64()).into(),
                ))
                .absolute()
                .top(px(REMOVE_INSET))
                .right(px(REMOVE_INSET))
                .flex()
                .items_center()
                .justify_center()
                .size(px(REMOVE_SIZE))
                .rounded_full()
                .bg(rgb(pal.bg_surface))
                .border_1()
                .border_color(pal.border)
                .cursor_pointer()
                .tooltip(|_, cx| tooltip_view(TOOLTIP_REMOVE.into(), cx))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.remove_mention(id, index, cx);
                }))
                .child(icon(IconName::Close, ICON_XS, pal.text_muted));
            div()
                .relative()
                .flex_none()
                .child(mention_element(mention, THUMB_SIZE, pal))
                .child(remove)
        });
        Some(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_end()
                .gap(px(SPACE_2))
                .px(px(SPACE_1))
                .pt(px(SPACE_1))
                .children(chips)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{Image, ImageFormat as ClipboardFormat};
    use krad_domain::ChatImage;

    fn image_item(format: ClipboardFormat) -> ClipboardItem {
        ClipboardItem {
            entries: vec![ClipboardEntry::Image(Image::from_bytes(
                format,
                b"png".to_vec(),
            ))],
        }
    }

    #[test]
    fn only_a_clipboard_holding_just_an_image_pastes_as_one() {
        assert_eq!(
            clipboard_image(&image_item(ClipboardFormat::Png)),
            Some((ImageFormat::Png, b"png".to_vec()))
        );
        assert_eq!(clipboard_image(&image_item(ClipboardFormat::Tiff)), None);
        let mut with_text = image_item(ClipboardFormat::Png);
        with_text
            .entries
            .push(ClipboardEntry::String(gpui_kit::ClipboardString::new(
                "caption".to_owned(),
            )));
        assert_eq!(clipboard_image(&with_text), None);
    }

    #[test]
    fn a_mention_is_attached_once_and_named_by_its_file() {
        let shot = Mention::Image(ChatImage {
            path: PathBuf::from("/config/chat-images/6d61696e/3/1.png"),
            format: ImageFormat::Png,
        });
        let file = Mention::from(PathBuf::from("/repo/a.rs"));
        let mut attached = Vec::new();
        added(&mut attached, vec![shot.clone(), file.clone()]);
        added(&mut attached, vec![shot.clone()]);
        assert_eq!(attached, [shot, file]);
        assert_eq!(mention_name(Path::new("/repo/a.rs")), "a.rs");
    }
}
