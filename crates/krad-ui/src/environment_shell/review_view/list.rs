use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::review::{MergeRequest, ProjectPath};

use super::words::HostWords;
use super::Remote;
use super::{ci_glyph, RequestList, ReviewFilter, RowFacts};
use crate::components::{icon, Button, IconName};
use crate::environment_shell::dialog_parts::{segment, SegmentSize};
use crate::environment_shell::settings_view::segmented_track;
use crate::environment_shell::EnvironmentShell;
use crate::theme::{
    palette, row_fill, row_shadow, Palette, ICON, LINE_2XS, LINE_XS, RADIUS_MD, SPACE_0_5, SPACE_1,
    SPACE_1_5, SPACE_2, SPACE_3, TEXT_2XS, TEXT_SM, TEXT_XS,
};

const LIST_W: f32 = 300.0;
const ROW_PAD: f32 = 10.0;
const FACT_ICON: f32 = 12.0;
const PIPELINE_ICON: f32 = 13.0;
const BRANCH_ICON: f32 = 11.0;
const FILTER_COLUMNS: u16 = 3;
const TITLE_GAP: f32 = 1.0;
const FACT_GAP: f32 = 3.0;
const LIST_ID: &str = "review-list";
const ROW_PREFIX: &str = "review-request";
const FILTER_PREFIX: &str = "review-filter";
const BTN_REFRESH: &str = "review-refresh";
const EMPTY_TEXT: &str = "Nothing matches this filter in the group's project.";
const DRAFT_PREFIX: &str = "Draft: ";

impl EnvironmentShell {
    pub(super) fn request_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let words = self.review_words();
        let open = self.review.detail.as_ref().map(|detail| detail.request.iid);
        let list = self.review.list.as_ref();
        let rows: Vec<AnyElement> = match list.map(|list| &list.requests) {
            None | Some(Remote::Loading) => vec![list_note(words.loading, pal)],
            Some(Remote::Failed(reason)) => vec![list_note(reason, pal)],
            Some(Remote::Ready((_, requests))) if requests.is_empty() => {
                vec![list_note(words.empty, pal), list_note(EMPTY_TEXT, pal)]
            }
            Some(Remote::Ready((project, requests))) => requests
                .iter()
                .map(|request| {
                    let facts = list.and_then(|list| list.facts.get(&request.iid)).copied();
                    Self::request_row(project, request, facts, open == Some(request.iid), cx)
                })
                .collect(),
        };
        let project = match list.map(|list| &list.requests) {
            Some(Remote::Ready((project, _))) => Some(project.to_string()),
            _ => None,
        };
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(LIST_W))
            .h_full()
            .gap(px(SPACE_2))
            .py(px(SPACE_3))
            .px(px(SPACE_2))
            .child(Self::list_header(project, words, cx))
            .child(self.filter_track(cx))
            .child(
                div()
                    .id(LIST_ID)
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .gap(px(SPACE_0_5))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element()
    }

    fn list_header(
        project: Option<String>,
        words: &HostWords,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_1_5))
            .child(icon(IconName::PullRequest, ICON, pal.text_muted))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(TITLE_GAP))
                    .child(
                        div()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_XS))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(pal.text_strong))
                            .child(words.requests),
                    )
                    .children(project.map(|project| {
                        div()
                            .truncate()
                            .font_family(mono)
                            .text_size(px(TEXT_2XS))
                            .line_height(px(LINE_2XS))
                            .text_color(rgb(pal.text_muted))
                            .child(project)
                    })),
            )
            .child(
                Button::new(BTN_REFRESH)
                    .icon(IconName::Repeat)
                    .small()
                    .tooltip(words.reload)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.refresh_review(cx))),
            )
            .into_any_element()
    }

    fn filter_track(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.review.filter;
        let listed = self.review.list.as_ref().and_then(listed_count);
        let segments = ReviewFilter::ALL.into_iter().map(|filter| {
            let count = (filter == current).then_some(listed).flatten();
            segment(
                (FILTER_PREFIX, filter as u64),
                SharedString::from(filter.counted_label(count)),
                None,
                current == filter,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .justify_center()
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.review.filter = filter;
                cx.notify();
            }))
        });
        segmented_track(pal)
            .grid()
            .grid_cols(FILTER_COLUMNS)
            .mx(px(SPACE_1_5))
            .children(segments)
            .into_any_element()
    }

    fn request_row(
        project: &ProjectPath,
        request: &MergeRequest,
        facts: Option<RowFacts>,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let iid = request.iid;
        let draft = if request.draft && !request.title.starts_with(DRAFT_PREFIX) {
            DRAFT_PREFIX
        } else {
            ""
        };
        let project = project.clone();
        div()
            .id((ROW_PREFIX, iid.0))
            .flex()
            .flex_col()
            .gap(px(SPACE_1_5))
            .p(px(ROW_PAD))
            .rounded(px(RADIUS_MD))
            .cursor_pointer()
            .when(selected, |row| {
                row.bg(row_fill(pal, cx)).shadow(row_shadow(pal, cx))
            })
            .when(!selected, |row| row.hover(|style| style.bg(pal.bg_hover)))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.open_merge_request(project.clone(), iid, window, cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(SPACE_1_5))
                    .child(
                        div()
                            .flex_none()
                            .font_family(mono.clone())
                            .text_size(px(TEXT_2XS))
                            .line_height(px(LINE_XS))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(pal.text_muted))
                            .child(format!("!{iid}")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_XS))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(pal.text_strong))
                            .child(format!("{draft}{}", request.title)),
                    ),
            )
            .child(facts_line(request, facts, pal, mono))
            .into_any_element()
    }
}

fn facts_line(
    request: &MergeRequest,
    facts: Option<RowFacts>,
    pal: &Palette,
    mono: SharedString,
) -> AnyElement {
    let fact = |glyph: IconName, text: String| {
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(FACT_GAP))
            .child(icon(glyph, FACT_ICON, pal.text_muted))
            .child(text)
    };
    let approvals = facts.map(|facts| {
        let (given, needed) = facts.approvals;
        fact(IconName::ThumbsUp, format!("{given}/{needed}"))
    });
    let pipeline = facts.map(|facts| ci_glyph(facts.pipeline, PIPELINE_ICON, pal));
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_2))
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_muted))
        .child(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(FACT_GAP))
                .child(icon(IconName::GitBranch, BRANCH_ICON, pal.text_muted))
                .child(
                    div()
                        .truncate()
                        .font_family(mono)
                        .text_size(px(TEXT_2XS))
                        .line_height(px(LINE_2XS))
                        .child(request.source_branch.clone()),
                ),
        )
        .children(pipeline)
        .children(approvals)
        .child(fact(
            IconName::Comment,
            request.user_notes_count.to_string(),
        ))
        .into_any_element()
}

fn listed_count(list: &RequestList) -> Option<usize> {
    match &list.requests {
        Remote::Ready((_, requests)) => Some(requests.len()),
        Remote::Loading | Remote::Failed(_) => None,
    }
}

fn list_note(text: &str, pal: &Palette) -> AnyElement {
    div()
        .px(px(SPACE_2))
        .py(px(SPACE_1))
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_muted))
        .child(text.to_owned())
        .into_any_element()
}
