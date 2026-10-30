use gpui_kit::{
    div, px, AnyElement, ClipboardItem, Div, ElementId, InteractiveElement, IntoElement,
    ParentElement, SharedString, Stateful, StatefulInteractiveElement, Styled,
};
use krad_domain::TerminalId;

use crate::components::{icon, tooltip_view, IconName};
use crate::theme::{Palette, ICON_XS, RADIUS_XS, SPACE_1};

const ACTION_SIZE: f32 = 24.0;
const MESSAGE_GROUP_PREFIX: &str = "surface-message";

pub(super) fn message_group(id: TerminalId, index: usize) -> SharedString {
    SharedString::from(format!("{MESSAGE_GROUP_PREFIX}-{}-{index}", id.as_u64()))
}

pub(super) fn message_actions(
    group: SharedString,
    actions: impl IntoIterator<Item = AnyElement>,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1))
        .invisible()
        .group_hover(group, Styled::visible)
        .children(actions)
        .into_any_element()
}

pub(super) fn action_button(
    id: ElementId,
    shown: IconName,
    tooltip: &'static str,
    pal: &Palette,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(ACTION_SIZE))
        .rounded(px(RADIUS_XS))
        .cursor_pointer()
        .hover(|style| style.bg(pal.bg_hover))
        .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
        .child(icon(shown, ICON_XS, pal.text_muted))
}

pub(super) fn copy_button(
    id: ElementId,
    text: String,
    tooltip: &'static str,
    pal: &Palette,
) -> AnyElement {
    action_button(id, IconName::Copy, tooltip, pal)
        .on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        })
        .into_any_element()
}
