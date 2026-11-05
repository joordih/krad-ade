use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};

use super::super::super::dialog_parts::{sheet_footer, sheet_overlay, sheet_panel, sheet_title};
use super::super::super::EnvironmentShell;
use super::reach::MachineChoice;
use super::HostKindRef;
use crate::components::{icon, working_dots, Button, ButtonVariant, IconName};
use crate::theme::{
    palette, ICON_SM, LINE_SM, LINE_XS, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2, SPACE_5, SPACE_6,
    TEXT_SM, TEXT_XS,
};

const DIALOG_W: f32 = 440.0;
const TITLE_PREFIX: &str = "New environment on";
const SUBTITLE: &str = "Its groups, terminals and agents run on that machine";
const LOADING_TEXT: &str = "Looking for machines…";
const EMPTY_TEXT: &str = "No machines found";
const CANCEL_LABEL: &str = "Cancel";
const BTN_CLOSE: &str = "machine-dialog-close";
const BTN_CANCEL: &str = "machine-dialog-cancel";
const MACHINE_ID: &str = "machine-dialog-machine";
const LOADING_DOTS_KEY: u64 = 0;

pub(in crate::environment_shell) enum MachineList {
    Loading,
    Shown(Vec<MachineChoice>),
    Failed(String),
}

pub(in crate::environment_shell) struct MachineDialog {
    pub(in crate::environment_shell) kind: HostKindRef,
    title: String,
    list: MachineList,
}

impl MachineDialog {
    pub(super) fn loading(kind: HostKindRef, title: String) -> Self {
        Self {
            kind,
            title,
            list: MachineList::Loading,
        }
    }

    pub(super) fn show(&mut self, choices: Vec<MachineChoice>) {
        self.list = MachineList::Shown(choices);
    }

    pub(super) fn fail(&mut self, reason: String) {
        self.list = MachineList::Failed(reason);
    }
}

fn dismiss(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_host_dialog(cx);
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn machine_dialog_sheet(
        dialog: &MachineDialog,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let note = |text: String, color: u32| {
            div()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(color))
                .child(text)
                .into_any_element()
        };
        let rows: Vec<AnyElement> = match &dialog.list {
            MachineList::Loading => vec![div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_1_5))
                .child(working_dots(LOADING_DOTS_KEY, pal.text_muted))
                .child(note(LOADING_TEXT.to_owned(), pal.text_muted))
                .into_any_element()],
            MachineList::Failed(reason) => vec![note(reason.clone(), pal.error_fg)],
            MachineList::Shown(choices) if choices.is_empty() => {
                vec![note(EMPTY_TEXT.to_owned(), pal.text_muted)]
            }
            MachineList::Shown(choices) => choices
                .iter()
                .enumerate()
                .map(|(index, choice)| {
                    let available = choice.available();
                    let kind = dialog.kind.clone();
                    let machine = choice.machine.clone();
                    let detail = choice
                        .hint
                        .map(str::to_owned)
                        .or_else(|| choice.machine.description.clone());
                    div()
                        .id((MACHINE_ID, index))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_1_5))
                        .px(px(SPACE_2))
                        .py(px(SPACE_1))
                        .rounded(px(RADIUS_XS))
                        .text_size(px(TEXT_SM))
                        .line_height(px(LINE_SM))
                        .text_color(rgb(if available {
                            pal.text_default
                        } else {
                            pal.text_disabled
                        }))
                        .child(icon(IconName::Layers, ICON_SM, pal.text_muted))
                        .child(choice.machine.label.clone())
                        .children(detail.map(|detail| {
                            div()
                                .text_size(px(TEXT_XS))
                                .text_color(rgb(pal.text_muted))
                                .child(detail)
                        }))
                        .when(available, |row| {
                            row.cursor_pointer()
                                .hover(|style| style.bg(pal.bg_hover))
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.create_machine_environment(&kind, &machine, cx);
                                }))
                        })
                        .into_any_element()
                })
                .collect(),
        };
        let body = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .px(px(SPACE_6))
            .pb(px(SPACE_5))
            .children(rows);
        let sheet = sheet_panel(DIALOG_W, pal)
            .child(sheet_title(
                format!("{TITLE_PREFIX} {}", dialog.title),
                Some(SUBTITLE.to_owned()),
                BTN_CLOSE,
                dismiss,
                cx,
            ))
            .child(body)
            .child(
                sheet_footer(pal).justify_end().child(
                    Button::new(BTN_CANCEL)
                        .label(CANCEL_LABEL)
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.close_host_dialog(cx);
                        })),
                ),
            );
        sheet_overlay(sheet, dismiss, cx)
    }
}
