use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use super::super::context_menu::MenuPlacement;
use super::super::EnvironmentShell;
use crate::components::{icon, IconName};
use crate::theme::{
    palette, ICON_SM, ICON_XS, LINE_BASE, RADIUS_SM, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2,
    TEXT_BASE,
};

const BTN_ENVIRONMENT: &str = "sidebar-environment";
const PILL_H: f32 = 34.0;
const BADGE: f32 = 22.0;

impl EnvironmentShell {
    pub(super) fn environment_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let pill = div()
            .id(BTN_ENVIRONMENT)
            .flex()
            .flex_row()
            .flex_none()
            .w_full()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(PILL_H))
            .px(px(SPACE_1))
            .rounded(px(RADIUS_SM))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .active(|style| style.bg(pal.bg_selected))
            .on_click(
                cx.listener(|this: &mut Self, event: &gpui_kit::ClickEvent, _, cx| {
                    let at = event.position();
                    this.open_environment_menu(
                        (at.x.as_f32(), at.y.as_f32()),
                        MenuPlacement::Below,
                        cx,
                    );
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(BADGE))
                    .rounded(px(RADIUS_XS))
                    .bg(pal.bg_hover)
                    .child(icon(IconName::Layers, ICON_XS, pal.text_default)),
            )
            .child(
                div()
                    .text_size(px(TEXT_BASE))
                    .line_height(px(LINE_BASE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_strong))
                    .truncate()
                    .child(self.env_label(&self.active)),
            )
            .child(icon(IconName::ChevronDown, ICON_SM, pal.text_muted))
            .child(div().flex_1());
        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(SPACE_1_5))
            .pb(px(SPACE_2))
            .child(pill)
            .into_any_element()
    }
}
