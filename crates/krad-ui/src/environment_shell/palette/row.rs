use gpui_kit::component::command::CommandItem;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::{div, px, rgb, Action, IntoElement, ParentElement, SharedString, Styled};

use crate::actions::CANVAS_KEY_CONTEXT;
use crate::components::{icon, IconName};
use crate::theme::{palette, LINE_XS, SPACE_1, SPACE_2, SPACE_3, TEXT_XS};

const FOOTER_H: f32 = 32.0;
const RULE: f32 = 1.0;
const ROW_ICON: f32 = 15.0;
const FOOTER_KEYS: [(&str, &str); 3] = [("↑↓", "navigate"), ("↵", "run"), ("esc", "close")];

pub(super) type Glyph = (IconName, Option<u32>);

pub(super) fn command_row(
    label: impl Into<SharedString>,
    glyph: Glyph,
    shortcut: Option<Box<dyn Action>>,
) -> CommandItem {
    let label: SharedString = label.into();
    let shown = label.clone();
    CommandItem::new().label(label).child(move |window, cx| {
        let pal = palette(cx);
        let (name, color) = glyph;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .w_full()
            .min_w_0()
            .child(icon(name, ROW_ICON, color.unwrap_or(pal.text_muted)))
            .child(div().flex_1().min_w_0().truncate().child(shown.clone()))
            .children(shortcut.as_deref().and_then(|action| {
                Kbd::binding_for_action(action, Some(CANVAS_KEY_CONTEXT), window)
            }))
    })
}

pub(super) fn palette_footer(cx: &gpui_kit::App) -> impl IntoElement {
    let pal = palette(cx);
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_3))
        .h(px(FOOTER_H))
        .px(px(SPACE_3))
        .border_t(px(RULE))
        .border_color(pal.divider)
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .children(FOOTER_KEYS.iter().map(|(key, does)| {
            div()
                .flex()
                .flex_row()
                .gap(px(SPACE_1))
                .child(div().text_color(rgb(pal.text_default)).child(*key))
                .child(div().text_color(rgb(pal.text_muted)).child(*does))
        }))
}
