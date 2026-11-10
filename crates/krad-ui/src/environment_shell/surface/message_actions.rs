use gpui_kit::{
    div, px, AnyElement, App, Div, ElementId, Entity, InteractiveElement, IntoElement,
    ParentElement, SharedString, Stateful, StatefulInteractiveElement, Styled,
};
use krad_domain::TerminalId;

use crate::components::{
    copy_glyph, copy_with_feedback, icon, tooltip_view, CopyFeedback, IconName,
};
use crate::theme::{palette, Palette, ICON_XS, RADIUS_XS, SPACE_1};

const ACTION_SIZE: f32 = 24.0;
const MESSAGE_GROUP_PREFIX: &str = "surface-message";

pub(super) fn message_group(id: TerminalId, index: usize) -> SharedString {
    SharedString::from(format!("{MESSAGE_GROUP_PREFIX}-{}-{index}", id.as_u64()))
}

pub(super) struct BookmarkAction {
    pub(super) button: AnyElement,
    pub(super) marked: bool,
}

pub(super) fn message_actions(
    group: SharedString,
    actions: impl IntoIterator<Item = AnyElement>,
    bookmark: Option<BookmarkAction>,
) -> AnyElement {
    let hovered =
        |row: Div, group: SharedString| row.invisible().group_hover(group, Styled::visible);
    let bookmark = bookmark.map(|bookmark| {
        let slot = div().flex().child(bookmark.button);
        if bookmark.marked {
            slot
        } else {
            hovered(slot, group.clone())
        }
    });
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1))
        .child(
            hovered(
                div().flex().flex_row().items_center().gap(px(SPACE_1)),
                group,
            )
            .children(actions),
        )
        .children(bookmark)
        .into_any_element()
}

pub(super) fn action_button(
    id: ElementId,
    shown: IconName,
    tooltip: &'static str,
    pal: &Palette,
) -> Stateful<Div> {
    tinted_action_button(id, shown, tooltip, pal.text_muted, pal)
}

pub(super) fn tinted_action_button(
    id: ElementId,
    shown: IconName,
    tooltip: &'static str,
    color: u32,
    pal: &Palette,
) -> Stateful<Div> {
    action_shell(id, tooltip, pal).child(icon(shown, ICON_XS, color))
}

fn action_shell(id: ElementId, tooltip: &'static str, pal: &Palette) -> Stateful<Div> {
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
}

pub(super) fn copy_button(
    feedback: &Entity<CopyFeedback>,
    key: SharedString,
    text: String,
    tooltip: &'static str,
    cx: &App,
) -> AnyElement {
    let pal = palette(cx);
    let glyph = copy_glyph(feedback, &key, ICON_XS, pal.text_muted, cx);
    let feedback = feedback.clone();
    action_shell(ElementId::Name(key.clone()), tooltip, pal)
        .child(glyph)
        .on_click(move |_, _, cx| {
            copy_with_feedback(&feedback, key.clone(), text.clone(), cx);
        })
        .into_any_element()
}
