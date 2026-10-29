use gpui_kit::component::input::Input;
use gpui_kit::{div, px, rgb, AnyElement, Context, FontWeight, ParentElement, Styled, Window};

use super::super::dialog_parts::{sheet_footer, sheet_overlay, sheet_panel, sheet_title};
use super::super::EnvironmentShell;
use super::{DeleteTarget, NameDialog, NameDialogs, NameTarget};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_BASE, LINE_SM, SPACE_2, SPACE_5, SPACE_6, TEXT_BASE, TEXT_SM};

const DIALOG_W: f32 = 420.0;
const RENAME_ENVIRONMENT_TITLE: &str = "Rename environment";
const RENAME_GROUP_TITLE: &str = "Rename group";
const RENAME_CONFIRM: &str = "Rename";
const CANCEL_LABEL: &str = "Cancel";
const BTN_NAME_CLOSE: &str = "name-dialog-close";
const BTN_NAME_CANCEL: &str = "name-dialog-cancel";
const BTN_NAME_CONFIRM: &str = "name-dialog-confirm";
const BTN_DELETE_CLOSE: &str = "delete-dialog-close";
const BTN_DELETE_CANCEL: &str = "delete-dialog-cancel";
const BTN_DELETE_CONFIRM: &str = "delete-dialog-confirm";

fn dismiss(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_name_dialog(cx);
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn name_dialog_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match &self.name_dialogs {
            NameDialogs::Closed => None,
            NameDialogs::Name(dialog) => Some(Self::rename_sheet(dialog, cx)),
            NameDialogs::Delete(target) => Some(self.delete_sheet(target, cx)),
            NameDialogs::Look(session) => self.look_sheet(*session, cx),
        }
    }

    fn rename_sheet(dialog: &NameDialog, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let title = match dialog.target {
            NameTarget::Environment(_) => RENAME_ENVIRONMENT_TITLE,
            NameTarget::Group(_) => RENAME_GROUP_TITLE,
        };
        let sheet = sheet_panel(DIALOG_W, pal)
            .child(sheet_title(title, None, BTN_NAME_CLOSE, dismiss, cx))
            .child(
                div()
                    .px(px(SPACE_6))
                    .pb(px(SPACE_5))
                    .child(Input::new(&dialog.field)),
            )
            .child(
                sheet_footer(pal)
                    .justify_end()
                    .child(
                        Button::new(BTN_NAME_CANCEL)
                            .label(CANCEL_LABEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.close_name_dialog(cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_NAME_CONFIRM)
                            .label(RENAME_CONFIRM)
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.commit_name(cx);
                            })),
                    ),
            );
        sheet_overlay(sheet, dismiss, cx)
    }

    fn delete_sheet(&self, target: &DeleteTarget, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let confirmed = target.clone();
        let sheet = sheet_panel(DIALOG_W, pal)
            .child(sheet_title(
                target.title(),
                None,
                BTN_DELETE_CLOSE,
                dismiss,
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
                            .child(self.delete_subject(target)),
                    )
                    .child(
                        div()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .text_color(rgb(pal.text_muted))
                            .child(target.text()),
                    ),
            )
            .child(
                sheet_footer(pal)
                    .justify_end()
                    .child(
                        Button::new(BTN_DELETE_CANCEL)
                            .label(CANCEL_LABEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.close_name_dialog(cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_DELETE_CONFIRM)
                            .label(super::DELETE_CONFIRM)
                            .variant(ButtonVariant::Destructive)
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.delete(confirmed.clone(), cx);
                            })),
                    ),
            );
        sheet_overlay(sheet, dismiss, cx)
    }
}
