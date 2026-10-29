use gpui_kit::{
    div, px, rgb, AnyElement, App, FontWeight, IntoElement, ParentElement, RenderOnce,
    SharedString, Styled, Window,
};

use super::icon::{icon, IconName};
use crate::theme::{
    palette, ICON_LG, LINE_BASE, LINE_SM, SPACE_1, SPACE_2, SPACE_3, SPACE_6, TEXT_BASE, TEXT_SM,
};

const CENTERED_TEXT_MAX_W: f32 = 360.0;
const ICON_RING: f32 = 48.0;

#[derive(IntoElement)]
pub(crate) struct EmptyState {
    icon: IconName,
    title: SharedString,
    description: SharedString,
    action: Option<AnyElement>,
}

impl EmptyState {
    pub(crate) fn new(
        icon: IconName,
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
    ) -> Self {
        Self {
            icon,
            title: title.into(),
            description: description.into(),
            action: None,
        }
    }

    pub(crate) fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let pal = palette(cx);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(SPACE_1))
            .p(px(SPACE_6))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(ICON_RING))
                    .mb(px(SPACE_2))
                    .rounded_full()
                    .border_1()
                    .border_color(pal.divider)
                    .bg(pal.bg_hover)
                    .child(icon(self.icon, ICON_LG, pal.text_muted)),
            )
            .child(
                div()
                    .text_size(px(TEXT_BASE))
                    .line_height(px(LINE_BASE))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_strong))
                    .child(self.title),
            )
            .child(
                div()
                    .max_w(px(CENTERED_TEXT_MAX_W))
                    .text_center()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_subtle))
                    .child(self.description),
            )
            .children(
                self.action
                    .map(|action| div().mt(px(SPACE_3)).child(action)),
            )
    }
}

#[derive(IntoElement)]
pub(crate) struct CenteredEmpty {
    title: SharedString,
    description: SharedString,
    action: Option<AnyElement>,
}

impl CenteredEmpty {
    pub(crate) fn new(
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
    ) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            action: None,
        }
    }

    pub(crate) fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }
}

impl RenderOnce for CenteredEmpty {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let pal = palette(cx);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(SPACE_1))
            .p(px(SPACE_6))
            .child(
                div()
                    .text_size(px(TEXT_BASE))
                    .line_height(px(LINE_BASE))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_strong))
                    .child(self.title),
            )
            .child(
                div()
                    .max_w(px(CENTERED_TEXT_MAX_W))
                    .text_center()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_subtle))
                    .child(self.description),
            )
            .children(
                self.action
                    .map(|action| div().mt(px(SPACE_3)).child(action)),
            )
    }
}
