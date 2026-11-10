use gpui_kit::{
    div, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::review::Issue;
use krad_domain::{SessionTemplate, TEMPLATES_DIR};
use krad_repo::StoredTemplate;

use super::super::dialog_parts::{
    sheet_body, sheet_footer, sheet_group, sheet_overlay, sheet_panel, sheet_row_with,
    sheet_section, sheet_title,
};
use super::super::review_view::Remote;
use super::super::EnvironmentShell;
use super::{PickerStage, TemplatePicker};
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::theme::{palette, Palette, ICON_SM, LINE_SM, TEXT_SM};

const SHEET_W: f32 = 560.0;
const TEMPLATES_TITLE: &str = "New session";
const ISSUES_TITLE: &str = "Start from an issue";
const TEMPLATES_SECTION: &str = "Templates";
const ISSUES_SECTION: &str = "Open issues";
const BLANK_TITLE: &str = "Blank session";
const BLANK_DETAIL: &str = "A chat with the default model";
const BROKEN_DETAIL: &str = "Not a valid template: ";
const NEW_WORKTREE_DETAIL: &str = "new worktree";
const ASKS_ISSUE_DETAIL: &str = "asks for an issue";
const DETAIL_SEPARATOR: &str = " · ";
const LOADING_TEXT: &str = "Loading open issues…";
const NO_ISSUES_TEXT: &str = "The project has no open issues";
const ISSUE_MARK: &str = "#";
const FROM_ISSUE_LABEL: &str = "From an issue…";
const VARIANTS_LABEL: &str = "As variants…";
const BACK_LABEL: &str = "Back";
const CANCEL_LABEL: &str = "Cancel";
const BTN_CLOSE: &str = "template-picker-close";
const BTN_CANCEL: &str = "template-picker-cancel";
const BTN_FROM_ISSUE: &str = "template-picker-issue";
const BTN_VARIANTS: &str = "template-picker-variants";
const BTN_BACK: &str = "template-picker-back";
const BODY_ID: &str = "template-picker-body";
const BLANK_ROW: &str = "template-blank";
const TEMPLATE_ROW_PREFIX: &str = "template-row";
const ISSUE_ROW_PREFIX: &str = "template-issue";

fn close(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_template_picker(cx);
}

fn note(text: impl Into<SharedString>, pal: &Palette) -> AnyElement {
    div()
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(pal.text_subtle))
        .child(text.into())
        .into_any_element()
}

fn issue_labels(issue: &Issue) -> Option<SharedString> {
    (!issue.labels.is_empty()).then(|| issue.labels.join(DETAIL_SEPARATOR).into())
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn template_picker_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let picker = self.template_picker.as_ref()?;
        let pal = palette(cx);
        let sheet = match &picker.stage {
            PickerStage::Templates => sheet_panel(SHEET_W, pal)
                .child(sheet_title(
                    TEMPLATES_TITLE,
                    Some(TEMPLATES_DIR.to_owned()),
                    BTN_CLOSE,
                    close,
                    cx,
                ))
                .child(self.template_rows(picker, cx))
                .child(Self::templates_footer(cx)),
            PickerStage::Issues { template, issues } => sheet_panel(SHEET_W, pal)
                .child(sheet_title(
                    ISSUES_TITLE,
                    Some(template.name.clone()),
                    BTN_CLOSE,
                    close,
                    cx,
                ))
                .child(Self::issue_rows(issues, cx))
                .child(Self::issues_footer(cx)),
        };
        Some(sheet_overlay(sheet, close, cx))
    }

    fn template_detail(&self, template: &SessionTemplate) -> Option<SharedString> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(harness) = template.harness {
            parts.push(self.settings.acp.harness_name(harness).into_owned());
        }
        if let Some(model) = &template.model {
            parts.push(model.to_string());
        }
        if template.worktree.is_some() {
            parts.push(NEW_WORKTREE_DETAIL.to_owned());
        }
        if template.uses_issue() {
            parts.push(ASKS_ISSUE_DETAIL.to_owned());
        }
        (!parts.is_empty()).then(|| parts.join(DETAIL_SEPARATOR).into())
    }

    fn template_rows(&self, picker: &TemplatePicker, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let blank = sheet_row_with(
            Some(icon(IconName::Chat, ICON_SM, pal.text_subtle).into_any_element()),
            BLANK_TITLE,
            Some(BLANK_DETAIL.into()),
            None,
            pal,
        )
        .id(BLANK_ROW)
        .cursor_pointer()
        .on_click(cx.listener(|this: &mut Self, _, _, cx| this.start_blank_session(cx)))
        .into_any_element();
        let stored = picker
            .templates
            .iter()
            .enumerate()
            .map(|(index, stored)| self.stored_template_row(index, stored, cx));
        sheet_body(BODY_ID)
            .child(
                sheet_section(TEMPLATES_SECTION, None, pal)
                    .child(sheet_group(std::iter::once(blank).chain(stored), pal)),
            )
            .into_any_element()
    }

    fn stored_template_row(
        &self,
        index: usize,
        stored: &StoredTemplate,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        match &stored.template {
            Ok(template) => sheet_row_with(
                Some(icon(IconName::Layers, ICON_SM, pal.text_subtle).into_any_element()),
                template.name.clone(),
                self.template_detail(template),
                None,
                pal,
            )
            .id((TEMPLATE_ROW_PREFIX, index))
            .cursor_pointer()
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| this.start_template(index, cx)))
            .into_any_element(),
            Err(error) => sheet_row_with(
                Some(icon(IconName::CancelCircle, ICON_SM, pal.text_muted).into_any_element()),
                stored.id.to_string(),
                Some(format!("{BROKEN_DETAIL}{error}").into()),
                None,
                pal,
            )
            .into_any_element(),
        }
    }

    fn issue_rows(issues: &Remote<Vec<Issue>>, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let content = match issues {
            Remote::Loading => note(LOADING_TEXT, pal),
            Remote::Failed(error) => note(error.clone(), pal),
            Remote::Ready(issues) if issues.is_empty() => note(NO_ISSUES_TEXT, pal),
            Remote::Ready(issues) => {
                let rows = issues.iter().enumerate().map(|(index, issue)| {
                    sheet_row_with(
                        Some(icon(IconName::Flag, ICON_SM, pal.text_subtle).into_any_element()),
                        format!("{ISSUE_MARK}{} {}", issue.number, issue.title),
                        issue_labels(issue),
                        None,
                        pal,
                    )
                    .id((ISSUE_ROW_PREFIX, index))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.start_from_issue(index, cx);
                    }))
                    .into_any_element()
                });
                sheet_group(rows.collect::<Vec<_>>(), pal)
            }
        };
        sheet_body(BODY_ID)
            .child(sheet_section(ISSUES_SECTION, None, pal).child(content))
            .into_any_element()
    }

    fn cancel_button(cx: &mut Context<Self>) -> Button {
        Button::new(BTN_CANCEL)
            .label(CANCEL_LABEL)
            .variant(ButtonVariant::Secondary)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| this.close_template_picker(cx)))
    }

    fn templates_footer(cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        sheet_footer(pal)
            .child(
                Button::new(BTN_FROM_ISSUE)
                    .label(FROM_ISSUE_LABEL)
                    .icon(IconName::Flag)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.pick_issue_for(SessionTemplate::for_issue(), cx);
                    })),
            )
            .child(
                Button::new(BTN_VARIANTS)
                    .label(VARIANTS_LABEL)
                    .icon(IconName::Workflow)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.close_template_picker(cx);
                        this.open_variants_draft(window, cx);
                    })),
            )
            .child(div().flex_1())
            .child(Self::cancel_button(cx))
            .into_any_element()
    }

    fn issues_footer(cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        sheet_footer(pal)
            .child(
                Button::new(BTN_BACK)
                    .label(BACK_LABEL)
                    .icon(IconName::ChevronLeft)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.back_to_templates(cx);
                    })),
            )
            .child(div().flex_1())
            .child(Self::cancel_button(cx))
            .into_any_element()
    }
}
