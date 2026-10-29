use gpui_kit::{div, px, rgb, AnyElement, Context, FontWeight, ParentElement, Styled, Window};
use std::path::PathBuf;

use crate::components::{Button, ButtonVariant};
use crate::environment_shell::dialog_parts::{
    sheet_footer, sheet_overlay, sheet_panel, sheet_title,
};
use crate::environment_shell::native_alert::Confirmation;
use crate::environment_shell::EnvironmentShell;
use crate::theme::{palette, LINE_BASE, LINE_SM, SPACE_2, SPACE_5, SPACE_6, TEXT_BASE, TEXT_SM};

const DELETE_DIALOG_TITLE: &str = "Delete permanently?";
const DELETE_DIALOG_TEXT: &str =
    "This cannot be undone. Folders are deleted with everything in them.";
const DELETE_DIALOG_CANCEL: &str = "Cancel";
const DELETE_DIALOG_CONFIRM: &str = "Delete";
const BTN_DELETE_CANCEL: &str = "file-delete-cancel";
const BTN_DELETE_CONFIRM: &str = "file-delete-confirm";
const DELETE_DIALOG_W: f32 = 420.0;

const BTN_DELETE_CLOSE: &str = "delete-file-close";

fn cancel(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.file_tree.pending_delete = None;
    cx.notify();
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn ask_to_delete(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let detail = format!(
            "{}\n\n{DELETE_DIALOG_TEXT}",
            self.relative_to_root(&path).display()
        );
        let confirmation = Confirmation {
            title: DELETE_DIALOG_TITLE,
            detail: &detail,
            confirm: DELETE_DIALOG_CONFIRM,
            cancel: DELETE_DIALOG_CANCEL,
        };
        let target = path.clone();
        let asked = Self::confirm_natively(&confirmation, window, cx, |_, confirmed, _, cx| {
            if confirmed {
                Self::delete_file(target, cx);
            }
        });
        if !asked {
            self.file_tree.pending_delete = Some(path);
            cx.notify();
        }
    }

    pub(in crate::environment_shell) fn delete_file_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let path = self.file_tree.pending_delete.as_ref()?;
        let shown = self.relative_to_root(path).display().to_string();
        let pal = palette(cx);
        let sheet = sheet_panel(DELETE_DIALOG_W, pal)
            .child(sheet_title(
                DELETE_DIALOG_TITLE,
                None,
                BTN_DELETE_CLOSE,
                cancel,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_2))
                    .px(px(SPACE_6))
                    .pb(px(SPACE_5))
                    .child(
                        div()
                            .text_size(px(TEXT_BASE))
                            .line_height(px(LINE_BASE))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(pal.text_strong))
                            .child(shown),
                    )
                    .child(
                        div()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .text_color(rgb(pal.text_muted))
                            .child(DELETE_DIALOG_TEXT),
                    ),
            )
            .child(
                sheet_footer(pal)
                    .justify_end()
                    .child(
                        Button::new(BTN_DELETE_CANCEL)
                            .label(DELETE_DIALOG_CANCEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                                cancel(this, window, cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_DELETE_CONFIRM)
                            .label(DELETE_DIALOG_CONFIRM)
                            .variant(ButtonVariant::Destructive)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.confirm_file_delete(cx);
                            })),
                    ),
            );
        Some(sheet_overlay(sheet, cancel, cx))
    }
}
