use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled, Window,
};
use krad_domain::{MAX_VARIANTS, MIN_VARIANTS};

use super::super::dialog_parts::{
    field_label, segment, sheet_body, sheet_footer, sheet_overlay, sheet_panel, sheet_title,
    SegmentSize,
};
use super::super::review_view::Remote;
use super::super::EnvironmentShell;
use super::draft::{DraftRow, VariantDraft};
use crate::components::{harness_icon, icon, Button, ButtonVariant, IconName};
use crate::theme::{palette, ICON_SM, LINE_SM, SPACE_1, SPACE_2, SPACE_3, TEXT_SM};

const SHEET_W: f32 = 720.0;
const TITLE: &str = "Compare variants";
const SUBTITLE: &str = "The same task, run by several agents, each in its own worktree";
const PROMPT_LABEL: &str = "Task";
const VARIANTS_LABEL: &str = "Variants";
const TESTS_LABEL: &str = "Tests after each variant finishes";
const NO_TESTS_LABEL: &str = "None";
const LOADING_TASKS: &str = "Looking for project tasks…";
const NO_TASKS: &str = "The group has no project tasks";
const ADD_LABEL: &str = "Add a variant";
const LAUNCH_LABEL: &str = "Launch variants";
const CANCEL_LABEL: &str = "Cancel";
const BTN_CLOSE: &str = "variants-draft-close";
const BTN_ADD: &str = "variants-draft-add";
const BTN_LAUNCH: &str = "variants-draft-launch";
const BTN_CANCEL: &str = "variants-draft-cancel";
const BODY_ID: &str = "variants-draft-body";
const HARNESS_PREFIX: &str = "variants-harness";
const REMOVE_PREFIX: &str = "variants-remove";
const TASK_PREFIX: &str = "variants-task";
const NO_TASK_ID: &str = "variants-task-none";
const HARNESS_STRIDE: usize = 16;
const MODEL_FIELD_W: f32 = 180.0;

fn close(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_variants_ui(cx);
}

fn notice(text: &str, color: u32) -> AnyElement {
    div()
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(color))
        .child(text.to_owned())
        .into_any_element()
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn variants_draft_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let draft = self.variants.draft.as_ref()?;
        let pal = palette(cx);
        let sheet = sheet_panel(SHEET_W, pal)
            .child(sheet_title(
                TITLE,
                Some(SUBTITLE.to_owned()),
                BTN_CLOSE,
                close,
                cx,
            ))
            .child(
                sheet_body(BODY_ID)
                    .child(field_label(PROMPT_LABEL, pal))
                    .child(Textarea::new(&draft.prompt))
                    .child(field_label(VARIANTS_LABEL, pal))
                    .children(
                        draft
                            .rows
                            .iter()
                            .enumerate()
                            .map(|(index, row)| self.variant_draft_row(index, row, draft, cx)),
                    )
                    .child(field_label(TESTS_LABEL, pal))
                    .child(Self::draft_tasks(draft, cx)),
            )
            .child(Self::draft_footer(draft, cx));
        Some(sheet_overlay(sheet, close, cx))
    }

    fn variant_draft_row(
        &self,
        index: usize,
        row: &DraftRow,
        draft: &VariantDraft,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let harnesses = self
            .variant_harnesses()
            .into_iter()
            .enumerate()
            .map(|(at, preset)| {
                let (mark, _) = harness_icon(preset, pal);
                segment(
                    (HARNESS_PREFIX, index * HARNESS_STRIDE + at),
                    self.settings.acp.harness_name(preset),
                    Some(mark),
                    row.harness == preset,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    if let Some(row) = this
                        .variants
                        .draft
                        .as_mut()
                        .and_then(|draft| draft.rows.get_mut(index))
                    {
                        row.harness = preset;
                    }
                    cx.notify();
                }))
                .into_any_element()
            });
        let removable = draft.rows.len() > MIN_VARIANTS;
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .p(px(SPACE_2))
            .rounded(px(SPACE_2))
            .border_1()
            .border_color(pal.divider)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(SPACE_1))
                    .child(icon(IconName::Workflow, ICON_SM, pal.text_subtle))
                    .children(harnesses)
                    .child(div().flex_1())
                    .when(removable, |line| {
                        line.child(
                            Button::new((REMOVE_PREFIX, index))
                                .icon(IconName::Close)
                                .variant(ButtonVariant::Ghost)
                                .small()
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.remove_variant_row(index, cx);
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(SPACE_2))
                    .child(div().w(px(MODEL_FIELD_W)).child(Input::new(&row.model)))
                    .child(div().flex_1().child(Input::new(&row.instruction))),
            )
            .into_any_element()
    }

    fn draft_tasks(draft: &VariantDraft, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let tasks = match &draft.tasks {
            Remote::Loading => return notice(LOADING_TASKS, pal.text_subtle),
            Remote::Failed(error) => return notice(error, pal.text_subtle),
            Remote::Ready(tasks) if tasks.is_empty() => return notice(NO_TASKS, pal.text_subtle),
            Remote::Ready(tasks) => tasks,
        };
        let none = segment(
            NO_TASK_ID,
            NO_TESTS_LABEL,
            None,
            draft.test.is_none(),
            SegmentSize::Compact,
            pal,
            cx,
        )
        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
            if let Some(draft) = &mut this.variants.draft {
                draft.test = None;
            }
            cx.notify();
        }))
        .into_any_element();
        let choices = tasks.iter().enumerate().map(|(index, task)| {
            segment(
                (TASK_PREFIX, index),
                task.to_string(),
                Some(IconName::Play),
                draft.test == Some(index),
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                if let Some(draft) = &mut this.variants.draft {
                    draft.test = Some(index);
                }
                cx.notify();
            }))
            .into_any_element()
        });
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_1))
            .child(none)
            .children(choices)
            .into_any_element()
    }

    fn draft_footer(draft: &VariantDraft, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let full = draft.rows.len() >= MAX_VARIANTS;
        let message = draft
            .error
            .as_deref()
            .map(|error| notice(error, pal.error_fg))
            .or_else(|| {
                draft
                    .warning
                    .as_deref()
                    .map(|warning| notice(warning, pal.warning_fg))
            });
        div()
            .flex()
            .flex_col()
            .children(message.map(|message| div().px(px(SPACE_3)).pt(px(SPACE_2)).child(message)))
            .child(
                sheet_footer(pal)
                    .when(!full, |footer| {
                        footer.child(
                            Button::new(BTN_ADD)
                                .label(ADD_LABEL)
                                .icon(IconName::Add)
                                .variant(ButtonVariant::Secondary)
                                .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                                    this.add_variant_row(window, cx);
                                })),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        Button::new(BTN_CANCEL)
                            .label(CANCEL_LABEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.close_variants_ui(cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_LAUNCH)
                            .label(LAUNCH_LABEL)
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                                this.launch_drafted_variants(window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}
