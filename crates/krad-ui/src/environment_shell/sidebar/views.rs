use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled,
};

use super::super::top_bar::offered_views;
use super::super::EnvironmentShell;
use crate::components::icon;
use crate::theme::{
    palette, row_fill, row_shadow, ICON_SM, LINE_SM, NAV_ROW_H, RADIUS_XS, SPACE_0_5, SPACE_1,
    SPACE_2, TEXT_SM,
};

const VIEW_ROW_PREFIX: &str = "sidebar-view-";

impl EnvironmentShell {
    pub(super) fn view_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.active_view();
        let rows =
            offered_views(self.review_host().is_some())
                .into_iter()
                .map(|(view, glyph, label)| {
                    let selected = view == current;
                    div()
                        .id(ElementId::from(format!("{VIEW_ROW_PREFIX}{label}")))
                        .flex()
                        .flex_row()
                        .flex_none()
                        .items_center()
                        .gap(px(SPACE_2))
                        .h(px(NAV_ROW_H))
                        .px(px(SPACE_2))
                        .rounded(px(RADIUS_XS))
                        .cursor_pointer()
                        .text_size(px(TEXT_SM))
                        .line_height(px(LINE_SM))
                        .text_color(rgb(pal.text_strong))
                        .when(selected, |row| {
                            row.bg(row_fill(pal, cx))
                                .shadow(row_shadow(pal, cx))
                                .font_weight(FontWeight::MEDIUM)
                        })
                        .when(!selected, |row| row.hover(|style| style.bg(pal.bg_hover)))
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.set_view(view, cx);
                        }))
                        .child(icon(glyph, ICON_SM, pal.text_default))
                        .child(label)
                        .into_any_element()
                });
        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(SPACE_0_5))
            .py(px(SPACE_1))
            .border_b_1()
            .border_color(pal.divider)
            .children(rows)
            .into_any_element()
    }
}
