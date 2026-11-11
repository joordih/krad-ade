use gpui_kit::component::input::Textarea;
use gpui_kit::{
    div, px, rgb, AnyElement, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{AnchorState, DiffCommentId, DiffSide, LineSpan, TerminalId};

use super::super::review_view::DIFF_GUTTER_W;
use super::super::{EnvironmentShell, LiveEnv};
use super::comments::CommentPart;
use super::DIFF_LINE_H;
use crate::components::{icon, tooltip_view, Button, ButtonVariant, IconName};
use crate::theme::{
    level_fill, palette, tinted, SurfaceLevel, ICON_XS, LINE_SM, RADIUS_SM, RADIUS_XS, SPACE_1,
    SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM,
};

const BTN_COMMENT_PREFIX: &str = "changes-tab-comment";
const BTN_EDIT_PREFIX: &str = "changes-tab-comment-edit";
const BTN_DELETE_PREFIX: &str = "changes-tab-comment-delete";
const BTN_SAVE: &str = "changes-tab-comment-save";
const BTN_CANCEL: &str = "changes-tab-comment-cancel";
const BTN_SEND_REVIEW: &str = "changes-tab-send-review";
const TOOLTIP_COMMENT: &str = "Comment on this line (Shift-click to extend the range)";
const TOOLTIP_EDIT: &str = "Edit comment";
const TOOLTIP_DELETE: &str = "Delete comment";
const TOOLTIP_SEND_REVIEW: &str = "Send every comment of this session to its agent";
const SAVE_LABEL: &str = "Comment";
const CANCEL_LABEL: &str = "Cancel";
const SEND_REVIEW_LABEL: &str = "Send review";
const STALE_LABEL: &str = "Outdated";
const COMMENT_ON: &str = "Comment on";
const LINE_WORD: &str = "line";
const LINES_WORD: &str = "lines";
const OLD_SIDE_WORD: &str = "(previous version)";
const SELECTION_ALPHA: u32 = 0x33;
const COMMENT_ALPHA: u32 = 0x14;
const BUTTON_INSET: f32 = 2.0;

fn span_label(span: LineSpan) -> String {
    let lines = if span.first == span.last {
        format!("{LINE_WORD} {}", span.first)
    } else {
        format!("{LINES_WORD} {}-{}", span.first, span.last)
    };
    match span.side {
        DiffSide::New => lines,
        DiffSide::Old => format!("{lines} {OLD_SIDE_WORD}"),
    }
}

pub(super) fn comment_title(span: LineSpan) -> SharedString {
    format!("{COMMENT_ON} {}", span_label(span)).into()
}

impl EnvironmentShell {
    pub(super) fn comment_button(
        tab: TerminalId,
        line: usize,
        group: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        div()
            .id((BTN_COMMENT_PREFIX, line))
            .absolute()
            .left(px(DIFF_GUTTER_W))
            .top(px(BUTTON_INSET))
            .size(px(DIFF_LINE_H - 2.0 * BUTTON_INSET))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RADIUS_XS))
            .bg(rgb(pal.accent))
            .cursor_pointer()
            .invisible()
            .group_hover(group, Styled::visible)
            .tooltip(|_, cx| tooltip_view(TOOLTIP_COMMENT.into(), cx))
            .on_click(
                cx.listener(move |this: &mut Self, event: &ClickEvent, window, cx| {
                    this.start_comment(tab, line, event.modifiers().shift, window, cx);
                }),
            )
            .child(icon(IconName::Add, ICON_XS, pal.on_accent))
            .into_any_element()
    }

    pub(super) fn selection_wash(cx: &Context<Self>) -> gpui_kit::Rgba {
        tinted(palette(cx).accent, SELECTION_ALPHA)
    }

    pub(super) fn comment_row(
        live: &LiveEnv,
        tab: TerminalId,
        id: DiffCommentId,
        part: CommentPart,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let Some(comment) = live.tab_comment(tab, id) else {
            return div().h(px(DIFF_LINE_H)).into_any_element();
        };
        let row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(DIFF_LINE_H))
            .pl(px(DIFF_GUTTER_W))
            .pr(px(SPACE_3))
            .bg(tinted(pal.accent, COMMENT_ALPHA));
        let CommentPart::Text(text_row) = part else {
            let stale = (comment.state == AnchorState::Stale).then(|| {
                div()
                    .flex_none()
                    .text_color(rgb(pal.warning_fg))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(STALE_LABEL)
            });
            return row
                .text_color(rgb(pal.text_muted))
                .child(icon(IconName::Comment, ICON_XS, pal.accent))
                .child(div().flex_none().child(span_label(comment.quote.span)))
                .children(stale)
                .child(Self::comment_action(
                    (BTN_EDIT_PREFIX, id.0),
                    IconName::Pencil,
                    TOOLTIP_EDIT,
                    cx.listener(move |this: &mut Self, _: &ClickEvent, window, cx| {
                        this.edit_comment(tab, id, window, cx);
                    }),
                    cx,
                ))
                .child(Self::comment_action(
                    (BTN_DELETE_PREFIX, id.0),
                    IconName::Close,
                    TOOLTIP_DELETE,
                    cx.listener(move |this: &mut Self, _: &ClickEvent, _, cx| {
                        this.delete_comment(tab, id, cx);
                    }),
                    cx,
                ))
                .into_any_element();
        };
        let text = comment
            .text
            .lines()
            .nth(text_row)
            .unwrap_or_default()
            .to_owned();
        row.text_color(rgb(pal.text_strong))
            .child(text)
            .into_any_element()
    }

    fn comment_action(
        id: (&'static str, u64),
        glyph: IconName,
        tooltip: &'static str,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui_kit::App) + 'static,
        cx: &Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        div()
            .id(id)
            .flex_none()
            .size(px(DIFF_LINE_H - 2.0 * BUTTON_INSET))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
            .on_click(on_click)
            .child(icon(glyph, ICON_XS, pal.text_muted))
            .into_any_element()
    }

    pub(super) fn send_review_button(
        live: &LiveEnv,
        tab: TerminalId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let count = live.tab_review_size(tab);
        (count > 0).then(|| {
            Button::new(BTN_SEND_REVIEW)
                .icon(IconName::Send)
                .label(format!("{SEND_REVIEW_LABEL} ({count})"))
                .small()
                .variant(ButtonVariant::Primary)
                .tooltip(TOOLTIP_SEND_REVIEW)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.send_review(tab, cx);
                }))
                .into_any_element()
        })
    }

    pub(super) fn draft_strip(
        live: &LiveEnv,
        tab: TerminalId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let diff = live.diffs.get(&tab)?;
        let draft = diff.draft.as_ref()?;
        let pal = palette(cx);
        let title = draft.title.clone();
        Some(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap(px(SPACE_1))
                .p(px(SPACE_2))
                .border_t_1()
                .border_color(pal.divider)
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .children(title.map(|title| div().text_color(rgb(pal.text_muted)).child(title)))
                .child(
                    div()
                        .px(px(SPACE_2))
                        .py(px(SPACE_1))
                        .rounded(px(RADIUS_SM))
                        .border_1()
                        .border_color(pal.divider)
                        .bg(level_fill(SurfaceLevel::Pane, pal.bg_surface, cx))
                        .child(Textarea::new(&draft.field).appearance(false)),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap(px(SPACE_1))
                        .child(
                            Button::new(BTN_CANCEL)
                                .label(CANCEL_LABEL)
                                .small()
                                .variant(ButtonVariant::Ghost)
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.cancel_comment(tab, cx);
                                })),
                        )
                        .child(
                            Button::new(BTN_SAVE)
                                .icon(IconName::Comment)
                                .label(SAVE_LABEL)
                                .small()
                                .variant(ButtonVariant::Primary)
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.save_comment(tab, cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}
