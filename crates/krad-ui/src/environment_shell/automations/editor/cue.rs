use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{div, px, rgb, AnyElement, Context, ElementId, IntoElement, ParentElement, Styled};
use krad_domain::Trigger;

use super::super::super::EnvironmentShell;
use super::super::wording::cue_sentence;
use super::parts::{hint, panel};
use super::AutomationEditor;
use crate::components::motion::drop_in;
use crate::components::{icon, Button, IconName};
use crate::theme::{Palette, ICON_SM, SPACE_2, TEXT_XS};

const CUE_ENTER_PREFIX: &str = "automation-cue-enter";
const REMOVE_PREFIX: &str = "automation-cue-remove";
const BTN_ADD_CUE: &str = "automation-add-cue";
const ADD_CUE_LABEL: &str = "Add a cue";
const CLOSE_PICKER_LABEL: &str = "Done picking";
const CUE_DETAIL: &str =
    "Any one of these sets it off. Clock cues follow local time while the app is open.";
const NO_CUE: &str = "Nothing wakes it yet, so it would never run.";
pub(super) const REMOVE_TOOLTIP: &str = "Drop this cue";

const fn cue_glyph(trigger: Trigger) -> IconName {
    match trigger {
        Trigger::Schedule { .. } => IconName::Repeat,
        Trigger::App { .. } => IconName::Flash,
        Trigger::CodeHost { .. } => IconName::PullRequest,
        Trigger::Manual => IconName::Play,
    }
}

impl EnvironmentShell {
    pub(super) fn cue_step(
        &self,
        editor: &AutomationEditor,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let schedules: Vec<AnyElement> = editor
            .schedules
            .iter()
            .enumerate()
            .map(|(index, row)| Self::schedule_row(index, row, pal, cx))
            .collect();
        let others: Vec<AnyElement> = editor
            .draft
            .triggers
            .iter()
            .enumerate()
            .map(|(index, trigger)| Self::cue_row(index, *trigger, pal, cx))
            .collect();
        let empty = schedules.is_empty() && others.is_empty();
        let open = editor.picker.is_some();
        let add = Button::new(BTN_ADD_CUE)
            .label(if open {
                CLOSE_PICKER_LABEL
            } else {
                ADD_CUE_LABEL
            })
            .icon(if open {
                IconName::ChevronUp
            } else {
                IconName::Add
            })
            .small()
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.edit_automation_draft(cx, AutomationEditor::toggle_picker);
            }));
        let picker = editor
            .picker
            .map(|picker| self.cue_picker(editor, picker, pal, cx));
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .child(hint(CUE_DETAIL, pal))
            .children(schedules)
            .children(others)
            .when(empty, |column| {
                column.child(
                    div()
                        .text_size(px(TEXT_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(NO_CUE),
                )
            })
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_2))
                    .child(div().flex().flex_row().child(add))
                    .children(picker),
            )
            .into_any_element()
    }

    fn cue_row(
        index: usize,
        trigger: Trigger,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = panel(pal)
            .relative()
            .flex_row()
            .items_center()
            .child(icon(cue_glyph(trigger), ICON_SM, pal.text_subtle))
            .child(
                div()
                    .flex_1()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_strong))
                    .child(cue_sentence(trigger)),
            )
            .child(
                Button::new((REMOVE_PREFIX, index))
                    .icon(IconName::Close)
                    .small()
                    .tooltip(REMOVE_TOOLTIP)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.edit_automation_draft(cx, |editor| {
                            editor.draft.toggle_trigger(trigger);
                        });
                    })),
            );
        drop_in(
            ElementId::NamedInteger(
                CUE_ENTER_PREFIX.into(),
                u64::try_from(index).unwrap_or_default(),
            ),
            row,
        )
    }
}
