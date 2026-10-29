use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::review::MergeRequestState;

use super::detail::{RequestDetail, ReviewTab, HEADER_PAD_X};
use super::remote::Approval;
use super::summary::{approval_tally, short_time};
use super::Remote;
use super::{ci_color, ci_glyph, ci_label, state_color, state_label, status_pill};
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{
    palette, LINE_2XS, LINE_LG, LINE_XS, RADIUS_XS, SPACE_0_5, SPACE_1, SPACE_2, SPACE_3, TEXT_2XS,
    TEXT_LG, TEXT_SM, TEXT_XS,
};

const HEADER_PAD_TOP: f32 = 14.0;
const HEADER_GAP: f32 = 10.0;
const BRANCH_CHIP_H: f32 = 22.0;
const TAB_H: f32 = 34.0;
const TAB_PAD_X: f32 = 10.0;
const TAB_BAR: f32 = 2.0;
const TAB_GAP: f32 = 6.0;
const META_ICON: f32 = 13.0;
const FACT_ICON: f32 = 12.0;
const TAB_PREFIX: &str = "review-tab";
const BTN_OPEN_WEB: &str = "review-open-web";
const BTN_APPROVE: &str = "review-approve";
const BTN_MERGE: &str = "review-merge";
const APPROVE_LABEL: &str = "Approve";
const REVOKE_LABEL: &str = "Revoke approval";
const BRANCH_ARROW: &str = "→";

impl EnvironmentShell {
    pub(super) fn with_header(
        &self,
        detail: &RequestDetail,
        body: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .gap(px(HEADER_GAP))
                    .pt(px(HEADER_PAD_TOP))
                    .px(px(HEADER_PAD_X))
                    .child(self.title_row(detail, cx))
                    .child(Self::meta_row(detail, cx))
                    .child(Self::tab_strip(detail, cx)),
            )
            .child(body)
            .into_any_element()
    }

    fn title_row(&self, detail: &RequestDetail, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let request = &detail.request;
        let open = request.state == MergeRequestState::Opened;
        let url = request.web_url.clone();
        let approve = open
            .then(|| detail.approval_action(self.gitlab_user()))
            .flatten()
            .map(|action| {
                let button = Button::new(BTN_APPROVE)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.set_approval(action, cx);
                    }));
                match action {
                    Approval::Approve => button.label(APPROVE_LABEL).icon(IconName::ThumbsUp),
                    Approval::Revoke => button.label(REVOKE_LABEL),
                }
            });
        let merge = (detail.is_mergeable() && detail.tab != ReviewTab::Overview).then(|| {
            Button::new(BTN_MERGE)
                .label(detail.merge_label())
                .icon(IconName::Merge)
                .small()
                .variant(ButtonVariant::Primary)
                .on_click(cx.listener(|this: &mut Self, _, _, cx| this.merge_request(cx)))
        });
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(HEADER_GAP))
            .child(
                div()
                    .flex_none()
                    .font_family(mono)
                    .text_size(px(TEXT_SM))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_muted))
                    .child(format!("!{}", request.iid)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(TEXT_LG))
                    .line_height(px(LINE_LG))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_strong))
                    .child(request.title.clone()),
            )
            .child(
                Button::new(BTN_OPEN_WEB)
                    .label(detail.words.open_web)
                    .icon(IconName::OpenExternal)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            )
            .children(approve)
            .children(merge)
            .into_any_element()
    }

    fn meta_row(detail: &RequestDetail, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let request = &detail.request;
        let branch = |name: &str| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .h(px(BRANCH_CHIP_H))
                .px(px(SPACE_2))
                .rounded(px(RADIUS_XS))
                .bg(rgb(pal.bg_panel))
                .font_family(mono.clone())
                .text_size(px(TEXT_2XS))
                .line_height(px(LINE_2XS))
                .text_color(rgb(pal.text_default))
                .child(name.to_owned())
        };
        let pipeline = request.head_pipeline.as_ref().map(|pipeline| {
            let status = Some(pipeline.status);
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_1))
                .text_color(rgb(ci_color(status, pal)))
                .child(ci_glyph(status, META_ICON, pal))
                .child(format!(
                    "Pipeline {}",
                    ci_label(pipeline.status).to_lowercase()
                ))
        });
        let approvals = match &detail.approvals {
            Remote::Ready(approvals) => {
                let (given, needed) = approval_tally(approvals);
                Some(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_1))
                        .child(icon(IconName::ThumbsUp, FACT_ICON, pal.text_muted))
                        .child(format!("{given} of {needed} approvals")),
                )
            }
            Remote::Loading | Remote::Failed(_) => None,
        };
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(SPACE_2))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .text_color(rgb(pal.text_muted))
            .child(status_pill(
                state_label(request.state),
                state_color(request.state, pal),
            ))
            .child(branch(&request.source_branch))
            .child(BRANCH_ARROW)
            .child(branch(&request.target_branch))
            .child(format!(
                "by @{} · {}",
                request.author.username,
                short_time(&request.created_at)
            ))
            .children(pipeline)
            .children(approvals)
            .into_any_element()
    }

    fn tab_strip(detail: &RequestDetail, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let tabs = ReviewTab::ALL.into_iter().map(|tab| {
            let selected = tab == detail.tab;
            let count = detail.count(tab);
            div()
                .id((TAB_PREFIX, tab as u64))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(TAB_GAP))
                .h(px(TAB_H))
                .px(px(TAB_PAD_X))
                .cursor_pointer()
                .text_size(px(TEXT_SM))
                .text_color(rgb(if selected {
                    pal.text_strong
                } else {
                    pal.text_default
                }))
                .when(selected, |tab| {
                    tab.font_weight(FontWeight::SEMIBOLD)
                        .border_b(px(TAB_BAR))
                        .border_color(rgb(pal.text_strong))
                })
                .when(!selected, |tab| {
                    tab.hover(|style| style.text_color(rgb(pal.text_strong)))
                })
                .child(tab.label())
                .children(count.map(|count| {
                    div()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(rgb(pal.text_muted))
                        .child(count.to_string())
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    if let Some(detail) = this.review.detail.as_mut() {
                        detail.tab = tab;
                    }
                    cx.notify();
                }))
        });
        div()
            .flex()
            .flex_row()
            .gap(px(SPACE_0_5))
            .mx(px(-HEADER_PAD_X))
            .px(px(SPACE_3))
            .border_b_1()
            .border_color(pal.divider)
            .children(tabs)
            .into_any_element()
    }
}
