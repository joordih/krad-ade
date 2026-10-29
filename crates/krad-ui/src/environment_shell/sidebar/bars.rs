use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, Action, AnyElement, ClickEvent, Context, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Window,
};

use super::super::EnvironmentShell;
use super::{FOOTER_H, SIDEBAR_TOGGLE_END};
use crate::actions::OpenSettings;
use crate::components::{
    agent_status, icon, tooltip_view, tooltip_with_shortcut, IconName, StatusKind,
};
use crate::theme::{
    palette, row_fill, row_shadow, ICON_BUTTON_SM, ICON_SM, LINE_XS, RADIUS_XS, SPACE_0_5, SPACE_1,
    SPACE_2, TEXT_XS, WINDOW_INSET,
};
use crate::title_bar::WindowTitleBar;

const SIDEBAR_HEADER_ID: &str = "sidebar-header";
const TALLY_ICON_GAP: f32 = 3.0;
const BTN_SETTINGS: &str = "sidebar-settings";
const BTN_TERMINAL: &str = "sidebar-terminal";
const TOOLTIP_TERMINAL: &str = "New terminal in the utilities panel";
const TOOLTIP_SETTINGS: &str = "Settings";

impl EnvironmentShell {
    pub(super) fn sidebar_header() -> WindowTitleBar {
        WindowTitleBar::new(SIDEBAR_HEADER_ID)
            .without_controls()
            .padding(SPACE_1)
            .leading(SIDEBAR_TOGGLE_END - WINDOW_INSET)
            .child(div().flex_1())
    }

    fn footer_toggle(
        id: &'static str,
        glyph: IconName,
        selected: bool,
        tooltip: &'static str,
        on_click: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        shortcut: Option<Box<dyn Action>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .size(px(ICON_BUTTON_SM))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .when(selected, |button| {
                button.bg(row_fill(pal, cx)).shadow(row_shadow(pal, cx))
            })
            .when(!selected, |button| {
                button.hover(|style| style.bg(pal.bg_hover))
            })
            .tooltip(move |_, cx| match shortcut.as_deref() {
                Some(action) => tooltip_with_shortcut(tooltip.into(), action, cx),
                None => tooltip_view(tooltip.into(), cx),
            })
            .on_click(cx.listener(on_click))
            .child(icon(glyph, ICON_SM, pal.text_default))
            .into_any_element()
    }

    fn footer_tools(cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .gap(px(SPACE_0_5))
            .child(Self::footer_toggle(
                BTN_TERMINAL,
                IconName::Terminal,
                false,
                TOOLTIP_TERMINAL,
                |this, _, window, cx| this.new_utility_terminal(window, cx),
                None,
                cx,
            ))
            .child(Self::footer_toggle(
                BTN_SETTINGS,
                IconName::Settings,
                false,
                TOOLTIP_SETTINGS,
                |this, _, window, cx| this.open_settings(window, cx),
                Some(Box::new(OpenSettings)),
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn sidebar_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let tally = self
            .envs
            .get(&self.active)
            .map_or_default(|live| live.data.tally());
        let counts = tally.active_counts().into_iter().map(|(state, count)| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(TALLY_ICON_GAP))
                .child(agent_status(StatusKind::Agent(state), ICON_SM, pal))
                .child(count.to_string())
        });
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(FOOTER_H))
            .gap(px(SPACE_2))
            .child(Self::footer_tools(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_2))
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_default))
                    .children(counts),
            )
            .into_any_element()
    }
}
