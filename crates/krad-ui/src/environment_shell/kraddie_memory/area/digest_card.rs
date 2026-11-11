use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, ElementId, FontWeight, IntoElement,
    ParentElement, Styled, Window,
};
use krad_domain::{relative_age, ChatDigest, DigestAuthor, DigestId, MemoryOwner};
use std::time::SystemTime;

use super::super::super::EnvironmentShell;
use super::super::DigestEdit;
use crate::components::motion::{drop_in, rise_in};
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{
    level_fill, palette, Palette, SurfaceLevel, LINE_SM, LINE_XS, RADIUS_MD, SPACE_1, SPACE_1_5,
    SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};

const DIGEST_PREFIX: &str = "kraddie-digest";
const EDIT_PREFIX: &str = "kraddie-digest-edit";
const DELETE_PREFIX: &str = "kraddie-digest-delete";
const SAVE_PREFIX: &str = "kraddie-digest-save";
const CANCEL_PREFIX: &str = "kraddie-digest-cancel";
const EDIT_LABEL: &str = "Edit";
const DELETE_LABEL: &str = "Forget";
const SAVE_LABEL: &str = "Keep";
const CANCEL_LABEL: &str = "Cancel";
const EDITED_MARK: &str = " · edited by you";
const AGO_SUFFIX: &str = " ago";
const FACT_BULLET: &str = "•";
const EDIT_ROWS: usize = 6;

fn facts_list(facts: &[String], pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_1))
        .children(facts.iter().map(|fact| {
            div()
                .flex()
                .flex_row()
                .gap(px(SPACE_1_5))
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(pal.text_default))
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(pal.text_muted))
                        .child(FACT_BULLET),
                )
                .child(div().flex_1().min_w_0().child(fact.clone()))
        }))
        .into_any_element()
}

impl EnvironmentShell {
    pub(super) fn digest_card(
        &self,
        owner: &MemoryOwner,
        digest: &ChatDigest,
        index: usize,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let key = format!("{owner}-{}", digest.id.0);
        let age = SystemTime::now()
            .duration_since(digest.written)
            .unwrap_or_default();
        let mut stamp = format!("{}{AGO_SUFFIX}", relative_age(age));
        if digest.author == DigestAuthor::User {
            stamp.push_str(EDITED_MARK);
        }
        let editing = self
            .kraddies
            .memory
            .editing
            .as_ref()
            .filter(|edit| edit.owner == *owner && edit.digest == digest.id);
        let body = match editing {
            Some(edit) => Self::digest_editor(&key, edit, cx),
            None => facts_list(&digest.facts, pal),
        };
        let actions = editing
            .is_none()
            .then(|| Self::digest_actions(owner, digest, &key, cx));
        let card = div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(width))
            .min_w_0()
            .gap(px(SPACE_2))
            .p(px(SPACE_3))
            .rounded(px(RADIUS_MD))
            .border_1()
            .border_color(pal.border)
            .bg(level_fill(SurfaceLevel::Pane, pal.bg_panel, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(TEXT_SM))
                                    .line_height(px(LINE_SM))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(pal.text_strong))
                                    .child(digest.chat.title.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(TEXT_XS))
                                    .line_height(px(LINE_XS))
                                    .text_color(rgb(pal.text_muted))
                                    .child(stamp),
                            ),
                    )
                    .children(actions),
            )
            .child(body);
        rise_in(
            ElementId::Name(format!("{DIGEST_PREFIX}-{key}").into()),
            index,
            card,
        )
    }

    fn digest_editor(key: &str, edit: &DigestEdit, cx: &mut Context<Self>) -> AnyElement {
        let footer = div()
            .flex()
            .flex_row()
            .justify_end()
            .gap(px(SPACE_1))
            .child(
                Button::new(ElementId::Name(format!("{CANCEL_PREFIX}-{key}").into()))
                    .label(CANCEL_LABEL)
                    .small()
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.kraddies.memory.editing = None;
                        cx.notify();
                    })),
            )
            .child(
                Button::new(ElementId::Name(format!("{SAVE_PREFIX}-{key}").into()))
                    .label(SAVE_LABEL)
                    .variant(ButtonVariant::Primary)
                    .small()
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.save_digest_edit(cx))),
            );
        drop_in(
            ElementId::Name(format!("{EDIT_PREFIX}-open-{key}").into()),
            div()
                .relative()
                .flex()
                .flex_col()
                .gap(px(SPACE_2))
                .child(Textarea::new(&edit.text))
                .child(footer),
        )
    }

    fn digest_actions(
        owner: &MemoryOwner,
        digest: &ChatDigest,
        key: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (edit_owner, delete_owner) = (owner.clone(), owner.clone());
        let id = digest.id;
        let text = digest.text();
        div()
            .flex()
            .flex_row()
            .gap(px(SPACE_1))
            .child(
                Button::new(ElementId::Name(format!("{EDIT_PREFIX}-{key}").into()))
                    .label(EDIT_LABEL)
                    .icon(IconName::Pencil)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.begin_digest_edit(&edit_owner, id, &text, window, cx);
                    })),
            )
            .child(
                Button::new(ElementId::Name(format!("{DELETE_PREFIX}-{key}").into()))
                    .label(DELETE_LABEL)
                    .icon(IconName::Close)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.forget_digest(&delete_owner, id, cx);
                    })),
            )
            .into_any_element()
    }

    fn begin_digest_edit(
        &mut self,
        owner: &MemoryOwner,
        digest: DigestId,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = text.to_owned();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(EDIT_ROWS)
                .default_value(text)
        });
        self.kraddies.memory.editing = Some(DigestEdit {
            owner: owner.clone(),
            digest,
            text: input,
        });
        cx.notify();
    }

    fn save_digest_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.kraddies.memory.editing.take() else {
            return;
        };
        let text = edit.text.read(cx).value().to_string();
        let edited = self
            .kraddies
            .memory
            .book_mut(&edit.owner)
            .edit(edit.digest, &text);
        match edited {
            Ok(()) => self.save_digests(&edit.owner, cx),
            Err(error) => {
                self.toast(&error.to_string(), cx);
                self.kraddies.memory.editing = Some(edit);
            }
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn forget_digest(
        &mut self,
        owner: &MemoryOwner,
        digest: DigestId,
        cx: &mut Context<Self>,
    ) {
        if self.kraddies.memory.book_mut(owner).remove(digest).is_ok() {
            tracing::info!(%owner, digest = digest.0, "chat digest forgotten");
            self.save_digests(owner, cx);
        }
        cx.notify();
    }
}
