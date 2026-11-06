use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    anchored, deferred, div, point, px, relative, Anchor, AnyElement, Context, ElementId,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window,
};
use krad_extensions::api::manifest::TitlebarSide;

use super::super::super::rename::KEY_ESCAPE;
use super::super::super::top_bar::PANEL_BUTTON_RADIUS;
use super::super::super::EnvironmentShell;
use super::super::commands::icon_of;
use super::super::OpenViewId;
use super::ShownButton;
use crate::components::{icon, tooltip_view};
use crate::theme::{
    palette, popover_fill, popover_shadow, ICON, ICON_BUTTON, RADIUS_XL, SPACE_0_5, SPACE_1,
    TOOLBAR_H, WINDOW_INSET,
};

const BUTTON_PREFIX_LEFT: &str = "titlebar-extension-left";
const BUTTON_PREFIX_RIGHT: &str = "titlebar-extension-right";
const POPOVER_ID: &str = "titlebar-extension-popover";
const POPOVER_MIN_W: f32 = 220.0;
const POPOVER_MAX_W: f32 = 420.0;
const POPOVER_MIN_H: f32 = 48.0;
const POPOVER_MAX_H: f32 = 560.0;
const POPOVER_GAP: f32 = SPACE_1;
const POPOVER_EDGE: f32 = WINDOW_INSET;
const POPOVER_PRIORITY: usize = 1;
const BUTTON_CENTER: f32 = 0.5;
const BUTTON_BOTTOM: f32 = 1.0;

pub(super) fn popover_max_height(window_h: f32) -> f32 {
    let room = window_h - TOOLBAR_H - POPOVER_GAP - 2.0 * POPOVER_EDGE;
    room.clamp(POPOVER_MIN_H, POPOVER_MAX_H)
}

const fn button_prefix(side: TitlebarSide) -> &'static str {
    match side {
        TitlebarSide::Left => BUTTON_PREFIX_LEFT,
        TitlebarSide::Right => BUTTON_PREFIX_RIGHT,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn titlebar_items(
        &self,
        side: TitlebarSide,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let buttons = self.titlebar_buttons(side);
        if buttons.is_empty() {
            return None;
        }
        let max_h = popover_max_height(self.root_size(window).1);
        let items: Vec<AnyElement> = buttons
            .into_iter()
            .enumerate()
            .map(|(index, button)| self.titlebar_button(side, index, &button, max_h, cx))
            .collect();
        Some(
            div()
                .flex()
                .flex_row()
                .flex_none()
                .gap(px(SPACE_0_5))
                .occlude()
                .children(items)
                .into_any_element(),
        )
    }

    fn titlebar_button(
        &self,
        side: TitlebarSide,
        index: usize,
        button: &ShownButton<'_>,
        max_h: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let popover = self.popover_of(button.extension, &button.item.id);
        let filled = button.active || popover.is_some();
        let tooltip = SharedString::from(button.title.to_owned());
        let pressed = (button.extension.clone(), button.item.id.clone());
        div()
            .id(ElementId::from((button_prefix(side), index)))
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .size(px(ICON_BUTTON))
            .rounded(px(PANEL_BUTTON_RADIUS))
            .cursor_pointer()
            .when(filled, |button| button.bg(pal.bg_selected))
            .when(!filled, |button| {
                button.hover(|style| style.bg(pal.bg_hover))
            })
            .tooltip(move |_, cx| tooltip_view(tooltip.clone(), cx))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.press_titlebar_item(&pressed.0, &pressed.1, cx);
            }))
            .child(icon(icon_of(Some(button.icon)), ICON, pal.text_muted))
            .children(popover.map(|id| self.titlebar_popover(id, max_h, cx)))
            .into_any_element()
    }

    fn titlebar_popover(&self, id: OpenViewId, max_h: f32, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let focus = self
            .extensions
            .titlebar
            .popover
            .as_ref()
            .map(|popover| popover.focus.clone());
        let panel = div()
            .id(POPOVER_ID)
            .flex()
            .flex_col()
            .min_w(px(POPOVER_MIN_W))
            .max_w(px(POPOVER_MAX_W))
            .min_h(px(POPOVER_MIN_H))
            .max_h(px(max_h))
            .overflow_y_scroll()
            .rounded(px(RADIUS_XL))
            .bg(popover_fill(pal, cx))
            .border_1()
            .border_color(pal.divider)
            .shadow(popover_shadow(pal, cx))
            .occlude()
            .when_some(focus, |panel, focus| panel.track_focus(&focus))
            .on_key_down(cx.listener(|this: &mut Self, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == KEY_ESCAPE {
                    cx.stop_propagation();
                    this.close_extension_popover(cx);
                }
            }))
            .child(self.extension_view_body(id, cx));
        div()
            .absolute()
            .top(relative(BUTTON_BOTTOM))
            .left(relative(BUTTON_CENTER))
            .child(
                deferred(
                    anchored()
                        .anchor(Anchor::TopCenter)
                        .offset(point(px(0.0), px(POPOVER_GAP)))
                        .snap_to_window_with_margin(px(POPOVER_EDGE))
                        .child(panel),
                )
                .with_priority(POPOVER_PRIORITY),
            )
            .into_any_element()
    }

    pub(in crate::environment_shell) fn extension_popover_backdrop(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.extensions.titlebar.popover.as_ref()?;
        let dismiss = |this: &mut Self,
                       _: &gpui_kit::MouseDownEvent,
                       _: &mut Window,
                       cx: &mut Context<Self>| {
            this.close_extension_popover(cx);
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(dismiss))
                .on_mouse_down(MouseButton::Right, cx.listener(dismiss))
                .into_any_element(),
        )
    }
}
