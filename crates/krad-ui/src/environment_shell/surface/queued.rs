use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::{Prompt, TerminalId, TextScale};

use super::super::chat::question_bubble;
use super::super::EnvironmentShell;
use super::mentions::sent_mentions;
use super::shown_images::ShownImages;
use super::ChatView;
use crate::theme::{palette, LINE_2XS, SPACE_1, SPACE_1_5, TEXT_2XS};

const PENDING_OPACITY: f32 = 0.6;
const PENDING_BUBBLE_PREFIX: &str = "surface-pending-question";
const QUEUED_LABEL: &str = "Queued";
const CANCEL_LABEL: &str = "Cancel";
const CANCEL_PREFIX: &str = "surface-cancel";

impl ChatView {
    fn runtime_queued(&self) -> &[Prompt] {
        self.chat
            .structured()
            .map_or(&[], |structured| structured.runtime_queued())
    }

    pub(super) fn pending_len(&self) -> usize {
        self.runtime_queued().len()
    }

    pub(super) fn pending(&self, index: usize) -> Option<&Prompt> {
        self.runtime_queued().get(index)
    }

    pub(super) fn show_queued(&mut self, prompt: Prompt) {
        if let Some(structured) = self.chat.structured_mut() {
            structured.show_queued(prompt);
        }
    }
}

impl EnvironmentShell {
    pub(super) fn pending_bubble(
        id: TerminalId,
        (index, prompt): (usize, &Prompt),
        images: &ShownImages,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let raw = u64::try_from(index).unwrap_or(u64::MAX);
        div()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(SPACE_1))
            .opacity(PENDING_OPACITY)
            .when(!prompt.text.is_empty(), |bubble| {
                bubble.child(question_bubble(
                    ElementId::Name(
                        format!("{PENDING_BUBBLE_PREFIX}-{}-{raw}", id.as_u64()).into(),
                    ),
                    &prompt.text,
                    false,
                    TextScale::DEFAULT,
                    pal,
                    cx,
                ))
            })
            .children(sent_mentions(&prompt.mentions, images, pal, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .text_size(px(TEXT_2XS))
                    .line_height(px(LINE_2XS))
                    .text_color(rgb(pal.text_muted))
                    .child(QUEUED_LABEL)
                    .child(
                        div()
                            .id(ElementId::Name(SharedString::from(format!(
                                "{CANCEL_PREFIX}-{}-{raw}",
                                id.as_u64()
                            ))))
                            .cursor_pointer()
                            .text_color(rgb(pal.text_default))
                            .hover(|style| style.text_color(rgb(pal.text_strong)))
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.cancel_queued(id, index, cx);
                            }))
                            .child(CANCEL_LABEL),
                    ),
            )
            .into_any_element()
    }
}
