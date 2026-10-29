use gpui_kit::component::input::Textarea;
use gpui_kit::component::text::TextView;
use gpui_kit::{
    div, px, rgb, AnyElement, ClickEvent, Context, Div, ElementId, FontWeight, InteractiveElement,
    IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::review::{MergeRequestState, Pipeline};
use std::time::Duration;

use super::detail::RequestDetail;
use super::summary::{run_time, stages};
use super::threads::{comments, ThreadPlace};
use super::Remote;
use super::{ci_glyph, merge_status_label, meta_line, WASH_ALPHA};
use crate::components::{check_box, icon, Button, ButtonVariant, IconName};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{
    palette, row_shadow, tinted, Palette, BUTTON_H, ICON_SM, LINE_BASE, LINE_XS, RADIUS_LG,
    RADIUS_XL, SPACE_2, SPACE_3, TEXT_2XS, TEXT_BASE, TEXT_SM, TEXT_XS,
};

const SIDE_W: f32 = 320.0;
const COLUMN_GAP: f32 = 16.0;
const CARD_PAD: f32 = 14.0;
const CARD_GAP: f32 = 10.0;
const BADGE: f32 = 26.0;
const BADGE_RADIUS: f32 = 8.0;
const STAGE_ROW_H: f32 = 26.0;
const NOTE_LINE: f32 = 15.0;
const DESCRIPTION_ID: &str = "review-description";
const BTN_COMMENT: &str = "review-comment";
const BTN_SIDE_MERGE: &str = "review-side-merge";
const CHECK_REMOVE_BRANCH: &str = "review-remove-branch";
const CHECK_SQUASH: &str = "review-squash";
const DESCRIPTION_LABEL: &str = "Description";
const NO_DESCRIPTION: &str = "No description.";
const DISCUSSIONS_LABEL: &str = "Discussions";
const NO_DISCUSSIONS: &str = "No comments yet.";
const COMMENT_LABEL: &str = "Comment";
const READY_LABEL: &str = "Ready to merge";
const REMOVE_BRANCH_LABEL: &str = "Delete the source branch";
const NOT_LINKED_NOTE: &str =
    "Move its task to Done yourself after the merge; tasks are not linked to merge requests.";
const APPROVALS_LABEL: &str = "Approvals";
const NO_APPROVALS: &str = "No approvals yet.";
const YOU: &str = "You";
const READ_ONLY: &str = "read only";
const LOADING: &str = "Loading…";

impl EnvironmentShell {
    pub(super) fn overview(&self, detail: &RequestDetail, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_start()
            .gap(px(COLUMN_GAP))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(COLUMN_GAP))
                    .child(Self::description(detail, cx))
                    .child(Self::discussions(detail, cx))
                    .child(Self::comment_box(detail, cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .w(px(SIDE_W))
                    .gap(px(SPACE_3))
                    .children(Self::merge_card(detail, cx))
                    .child(self.approvals_card(detail, cx))
                    .child(Self::pipeline_card(detail, cx)),
            )
            .into_any_element()
    }

    fn description(detail: &RequestDetail, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let description = detail
            .request
            .description
            .clone()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| NO_DESCRIPTION.to_owned());
        section(DESCRIPTION_LABEL.to_owned(), pal)
            .child(
                div()
                    .text_size(px(TEXT_BASE))
                    .line_height(px(LINE_BASE))
                    .text_color(rgb(pal.text_strong))
                    .child(
                        TextView::markdown(ElementId::from(DESCRIPTION_ID), description)
                            .selectable(true),
                    ),
            )
            .into_any_element()
    }

    fn discussions(detail: &RequestDetail, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let Remote::Ready(discussions) = &detail.discussions else {
            let text = match &detail.discussions {
                Remote::Failed(reason) => reason.as_str(),
                Remote::Loading | Remote::Ready(_) => LOADING,
            };
            return section(DISCUSSIONS_LABEL.to_owned(), pal)
                .child(meta_line(text, pal))
                .into_any_element();
        };
        let unresolved = discussions
            .iter()
            .filter(|discussion| discussion.is_resolvable() && !discussion.is_resolved())
            .count();
        let threads: Vec<AnyElement> = discussions
            .iter()
            .enumerate()
            .filter(|(_, discussion)| comments(discussion).next().is_some())
            .map(|(index, discussion)| {
                Self::thread(index, discussion, detail, ThreadPlace::Overview, cx)
            })
            .collect();
        let label = if unresolved > 0 {
            format!("{DISCUSSIONS_LABEL} · {unresolved} unresolved")
        } else {
            DISCUSSIONS_LABEL.to_owned()
        };
        let section = section(label, pal);
        if threads.is_empty() {
            return section
                .child(meta_line(NO_DISCUSSIONS, pal))
                .into_any_element();
        }
        section.children(threads).into_any_element()
    }

    fn comment_box(detail: &RequestDetail, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .p(px(SPACE_2))
            .rounded(px(RADIUS_LG))
            .bg(rgb(pal.bg_surface))
            .shadow(row_shadow(pal, cx))
            .border_1()
            .border_color(pal.border)
            .child(Textarea::new(&detail.draft).appearance(false))
            .child(
                div().flex().flex_row().justify_end().child(
                    Button::new(BTN_COMMENT)
                        .label(COMMENT_LABEL)
                        .small()
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                            this.comment_on_request(window, cx);
                        })),
                ),
            )
            .into_any_element()
    }

    fn merge_card(detail: &RequestDetail, cx: &mut Context<Self>) -> Option<AnyElement> {
        if detail.request.state != MergeRequestState::Opened {
            return None;
        }
        let pal = palette(cx);
        let mergeable = detail.is_mergeable();
        let (title, badge) = if mergeable {
            (READY_LABEL, pal.success_fg)
        } else {
            (
                merge_status_label(detail.request.detailed_merge_status, detail.words),
                pal.text_muted,
            )
        };
        let squash_label = match &detail.commits {
            Remote::Ready(commits) if commits.len() == 1 => "Squash 1 commit".to_owned(),
            Remote::Ready(commits) => format!("Squash {} commits", commits.len()),
            Remote::Loading | Remote::Failed(_) => "Squash commits".to_owned(),
        };
        let merge = mergeable.then(|| {
            Button::new(BTN_SIDE_MERGE)
                .label(detail.merge_label())
                .variant(ButtonVariant::Primary)
                .on_click(cx.listener(|this: &mut Self, _, _, cx| this.merge_request(cx)))
        });
        let card = side_card(pal)
            .gap(px(CARD_GAP))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(BADGE))
                            .rounded(px(BADGE_RADIUS))
                            .bg(tinted(badge, WASH_ALPHA))
                            .child(icon(IconName::Merge, ICON_SM, badge)),
                    )
                    .child(
                        div()
                            .text_size(px(TEXT_SM))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(pal.text_strong))
                            .child(title),
                    ),
            )
            .child(merge_option(
                CHECK_REMOVE_BRANCH,
                REMOVE_BRANCH_LABEL.into(),
                detail.remove_source_branch,
                pal,
                cx.listener(|this: &mut Self, _, _, cx| {
                    if let Some(detail) = this.review.detail.as_mut() {
                        detail.remove_source_branch = !detail.remove_source_branch;
                    }
                    cx.notify();
                }),
            ))
            .child(merge_option(
                CHECK_SQUASH,
                squash_label.into(),
                detail.squash,
                pal,
                cx.listener(|this: &mut Self, _, _, cx| {
                    if let Some(detail) = this.review.detail.as_mut() {
                        detail.squash = !detail.squash;
                    }
                    cx.notify();
                }),
            ))
            .children(merge.map(|merge| div().flex().flex_col().h(px(BUTTON_H)).child(merge)))
            .child(
                div()
                    .text_size(px(TEXT_2XS))
                    .line_height(px(NOTE_LINE))
                    .text_color(rgb(pal.text_muted))
                    .child(NOT_LINKED_NOTE),
            );
        Some(card.into_any_element())
    }

    fn approvals_card(&self, detail: &RequestDetail, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let me = self.gitlab_user();
        let rows: Vec<AnyElement> = match &detail.approvals {
            Remote::Loading => vec![meta_line(LOADING, pal)],
            Remote::Failed(reason) => vec![meta_line(reason, pal)],
            Remote::Ready(approvals) if approvals.approved_by.is_empty() => {
                vec![meta_line(NO_APPROVALS, pal)]
            }
            Remote::Ready(approvals) => approvals
                .approved_by
                .iter()
                .map(|approval| {
                    let name = if Some(approval.user.id) == me {
                        YOU.to_owned()
                    } else {
                        format!("@{}", approval.user.username)
                    };
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_2))
                        .text_size(px(TEXT_XS))
                        .text_color(rgb(pal.text_strong))
                        .child(icon(IconName::ThumbsUp, ICON_SM, pal.success_fg))
                        .child(name)
                        .into_any_element()
                })
                .collect(),
        };
        side_card(pal)
            .gap(px(SPACE_2))
            .child(card_label(APPROVALS_LABEL.to_owned(), pal))
            .children(rows)
            .into_any_element()
    }

    fn pipeline_card(detail: &RequestDetail, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let Some(pipeline) = detail.request.head_pipeline.as_ref() else {
            return side_card(pal)
                .child(meta_line(detail.words.no_pipeline, pal))
                .into_any_element();
        };
        let rows: Vec<AnyElement> = match &detail.jobs {
            Remote::Loading => vec![meta_line(LOADING, pal)],
            Remote::Failed(reason) => vec![meta_line(reason, pal)],
            Remote::Ready(jobs) => stages(jobs)
                .into_iter()
                .map(|stage| {
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_2))
                        .h(px(STAGE_ROW_H))
                        .text_size(px(TEXT_XS))
                        .text_color(rgb(pal.text_strong))
                        .child(ci_glyph(Some(stage.status), ICON_SM, pal))
                        .child(div().flex_1().child(stage.name.clone()))
                        .child(div().text_color(rgb(pal.text_muted)).child(stage.meta()))
                        .into_any_element()
                })
                .collect(),
        };
        side_card(pal)
            .gap(px(CARD_GAP))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(card_label(format!("Pipeline #{}", pipeline.id), pal))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(TEXT_XS))
                            .text_color(rgb(pal.text_muted))
                            .child(pipeline_meta(pipeline)),
                    ),
            )
            .children(rows)
            .into_any_element()
    }
}

fn pipeline_meta(pipeline: &Pipeline) -> String {
    match pipeline.duration {
        Some(seconds) => format!("{READ_ONLY} · {}", run_time(Duration::from_secs(seconds))),
        None => READ_ONLY.to_owned(),
    }
}

fn merge_option(
    id: &'static str,
    label: SharedString,
    checked: bool,
    pal: &Palette,
    toggle: impl Fn(&ClickEvent, &mut Window, &mut gpui_kit::App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_2))
        .cursor_pointer()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_strong))
        .child(check_box(checked, pal))
        .child(label)
        .on_click(toggle)
        .into_any_element()
}

fn side_card(pal: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .p(px(CARD_PAD))
        .rounded(px(RADIUS_XL))
        .bg(rgb(pal.bg_panel))
}

fn card_label(text: String, pal: &Palette) -> Div {
    div()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(pal.text_subtle))
        .child(text)
}

fn section(label: String, pal: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_2))
        .child(card_label(label, pal))
}
