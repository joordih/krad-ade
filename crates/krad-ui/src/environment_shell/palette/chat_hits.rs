use gpui_kit::component::command::{CommandGroup, CommandItem};
use gpui_kit::{
    div, px, rgb, FontWeight, HighlightStyle, ParentElement, SharedString, Styled, StyledText,
};
use krad_domain::ChatHit;
use std::ops::Range;

use crate::components::{icon, IconName};
use crate::theme::{palette, LINE_XS, SPACE_2, TEXT_XS};

const PALETTE_GROUP_CHAT_HITS: &str = "In chats";
const ROW_ICON: f32 = 15.0;
const TITLE_SEPARATOR: &str = " \u{b7} ";

fn chat_hit_row(hit: &ChatHit, query: &str) -> CommandItem {
    let label: SharedString = format!("{}{TITLE_SEPARATOR}{}", hit.title, hit.snippet.text).into();
    let title: SharedString = hit.title.clone().into();
    let snippet: SharedString = hit.snippet.text.clone().into();
    let marks: Vec<Range<usize>> = hit.snippet.marks.clone();
    CommandItem::new()
        .label(label)
        .keywords([query.to_owned()])
        .child(move |_, cx| {
            let pal = palette(cx);
            let strong = HighlightStyle {
                color: Some(rgb(pal.text_strong).into()),
                font_weight: Some(FontWeight::BOLD),
                ..HighlightStyle::default()
            };
            let highlights: Vec<(Range<usize>, HighlightStyle)> =
                marks.iter().map(|mark| (mark.clone(), strong)).collect();
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_2))
                .w_full()
                .min_w_0()
                .child(icon(IconName::Chat, ROW_ICON, pal.text_muted))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(StyledText::new(snippet.clone()).with_highlights(highlights)),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(title.clone()),
                )
        })
}

pub(super) fn chat_hits_group(hits: &[ChatHit], query: &str) -> CommandGroup {
    CommandGroup::new()
        .label(PALETTE_GROUP_CHAT_HITS)
        .items(hits.iter().map(|hit| chat_hit_row(hit, query)))
}
