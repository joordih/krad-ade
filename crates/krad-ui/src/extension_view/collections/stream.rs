use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, Context, IntoElement, ParentElement, Styled, Window,
};
use krad_extensions::api::view::{Node, StreamLine};
use krad_extensions::api::NodeKey;
use std::ops::Range;

use super::{COLLECTION_MIN_H, STREAM_PREFIX};
use crate::extension_view::form::{element_id, stream_lines};
use crate::extension_view::style::tone_color;
use crate::extension_view::ExtensionView;
use crate::theme::{palette, Palette, LINE_XS, SPACE_2, TEXT_XS};

impl ExtensionView {
    pub(in crate::extension_view) fn stream(
        &self,
        node: &Node,
        lines: &[StreamLine],
        _pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = node.key.clone();
        let mono = cx.theme().mono_font_family.clone();
        let list = uniform_list(
            element_id(STREAM_PREFIX, &key),
            lines.len(),
            cx.processor(
                move |this: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                    this.stream_lines(&key, range, cx)
                },
            ),
        )
        .flex_1()
        .min_h(px(COLLECTION_MIN_H))
        .font_family(mono)
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS));
        match self.streams.get(&node.key) {
            Some(scroll) => list.track_scroll(scroll).into_any_element(),
            None => list.into_any_element(),
        }
    }

    fn stream_lines(
        &self,
        key: &NodeKey,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pal = palette(cx);
        let Some(lines) = stream_lines(&self.model.root, key) else {
            return Vec::new();
        };
        lines
            .get(range)
            .unwrap_or_default()
            .iter()
            .map(|line| {
                div()
                    .px(px(SPACE_2))
                    .whitespace_nowrap()
                    .text_color(rgb(tone_color(line.tone, pal)))
                    .child(line.text.clone())
                    .into_any_element()
            })
            .collect()
    }
}
