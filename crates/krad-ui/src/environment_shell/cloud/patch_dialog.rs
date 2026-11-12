use gpui_kit::{div, px, rgb, AnyElement, Context, ParentElement, Styled, Window};

use super::super::dialog_parts::{sheet_footer, sheet_overlay, sheet_panel, sheet_title};
use super::super::EnvironmentShell;
use super::sessions::{
    PatchAnswer, PATCH_CANCEL, PATCH_CONFIRM, PATCH_DETAIL, PATCH_LEAVE, PATCH_TITLE,
};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_SM, SPACE_5, SPACE_6, TEXT_SM};

const DIALOG_W: f32 = 420.0;
const BTN_CLOSE: &str = "cloud-patch-close";
const BTN_LEAVE: &str = "cloud-patch-leave";
const BTN_BRING: &str = "cloud-patch-bring";
const BTN_CANCEL: &str = "cloud-patch-cancel";

fn cancel(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.answer_asked_patch(PatchAnswer::Cancel, cx);
}

impl EnvironmentShell {
    fn answer_asked_patch(&mut self, answer: PatchAnswer, cx: &mut Context<Self>) {
        if let Some(question) = self.cloud.asking.take() {
            self.answer_patch(question, answer, cx);
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn cloud_patch_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.cloud.asking.as_ref()?;
        let pal = palette(cx);
        let sheet = sheet_panel(DIALOG_W, pal)
            .child(sheet_title(PATCH_TITLE, None, BTN_CLOSE, cancel, cx))
            .child(
                div()
                    .px(px(SPACE_6))
                    .pb(px(SPACE_5))
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_muted))
                    .child(PATCH_DETAIL),
            )
            .child(
                sheet_footer(pal)
                    .justify_end()
                    .child(
                        Button::new(BTN_CANCEL)
                            .label(PATCH_CANCEL)
                            .variant(ButtonVariant::Ghost)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.answer_asked_patch(PatchAnswer::Cancel, cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_LEAVE)
                            .label(PATCH_LEAVE)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.answer_asked_patch(PatchAnswer::Leave, cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_BRING)
                            .label(PATCH_CONFIRM)
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.answer_asked_patch(PatchAnswer::Bring, cx);
                            })),
                    ),
            );
        Some(sheet_overlay(sheet, cancel, cx))
    }
}
