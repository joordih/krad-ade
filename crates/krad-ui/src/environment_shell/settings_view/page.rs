use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, StatefulInteractiveElement, Styled,
};

use super::super::sidebar::SIDEBAR_WIDTH;
use super::super::EnvironmentShell;
use super::SettingsPane;
use crate::components::{card, icon, IconName};
use crate::keymap::OPEN_SETTINGS_HINT;
use crate::theme::{
    palette, row_fill, row_shadow, ICON_SM, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_XS, SPACE_0_5,
    SPACE_1, SPACE_2, SPACE_3, SPACE_6, TEXT_SM, TEXT_XS, WINDOW_INSET,
};
use crate::title_bar::WindowTitleBar;

const NAV_BAR_ID: &str = "settings-nav-bar";
const PAGE_BAR_ID: &str = "settings-page-bar";
const SETTINGS_SCROLL_ID: &str = "settings-page";
const BTN_BACK: &str = "settings-back";
const NAV_ROW_PREFIX: &str = "settings-nav";
const SETTINGS_TITLE: &str = "Settings";
const BACK_LABEL: &str = "Back";
const BREADCRUMB_SEPARATOR: &str = "/";
const ESCAPE_KEY: &str = "escape";
const PAGE_W: f32 = 720.0;
const PAGE_TITLE_TEXT: f32 = 22.0;
const PAGE_TITLE_LINE: f32 = 28.0;
const GROUP_LABEL_TOP: f32 = 14.0;
const PAGE_TOP: f32 = 20.0;
const PAGE_GAP: f32 = 24.0;

const GROUPS: [(&str, &[SettingsPane]); 2] = [
    ("App", &[SettingsPane::General, SettingsPane::Terminal]),
    (
        "Workspace",
        &[SettingsPane::Integrations, SettingsPane::Gateway],
    ),
];

impl EnvironmentShell {
    pub(in crate::environment_shell) fn settings_page(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let ui = self.settings_ui.as_ref()?;
        let content = match ui.pane {
            SettingsPane::General => self.general_pane(cx),
            SettingsPane::Terminal => self.terminal_pane(cx),
            SettingsPane::Integrations => self.integrations_pane(ui, cx),
            SettingsPane::Gateway => self.gateway_pane(&ui.gateway, cx),
        };
        Some(
            div()
                .id("settings")
                .relative()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .p(px(WINDOW_INSET))
                .gap(px(WINDOW_INSET))
                .on_key_down(
                    cx.listener(|this: &mut Self, event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key == ESCAPE_KEY {
                            cx.stop_propagation();
                            this.close_settings(window, cx);
                        }
                    }),
                )
                .child(Self::settings_nav(ui.pane, cx))
                .child(Self::settings_main(ui.pane, content, cx))
                .children(self.gitlab_device_dialog(cx))
                .into_any_element(),
        )
    }

    fn settings_nav(shown: SettingsPane, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let mut items: Vec<AnyElement> = Vec::new();
        for (label, panes) in GROUPS {
            items.push(
                div()
                    .px(px(SPACE_2))
                    .pt(px(GROUP_LABEL_TOP))
                    .pb(px(SPACE_1))
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_muted))
                    .child(label)
                    .into_any_element(),
            );
            items.extend(
                panes
                    .iter()
                    .map(|pane| Self::nav_row(*pane, *pane == shown, cx)),
            );
        }
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .child(
                WindowTitleBar::new(NAV_BAR_ID)
                    .without_controls()
                    .padding(SPACE_1)
                    .child(div().flex_1()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .px(px(SPACE_1))
                    .gap(px(SPACE_0_5))
                    .children(items),
            )
            .child(
                div()
                    .id(BTN_BACK)
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_2))
                    .h(px(NAV_ROW_H))
                    .mx(px(SPACE_1))
                    .mb(px(SPACE_1))
                    .px(px(SPACE_2))
                    .rounded(px(RADIUS_XS))
                    .cursor_pointer()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_default))
                    .hover(|style| style.bg(pal.bg_hover))
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.close_settings(window, cx);
                    }))
                    .child(icon(IconName::ChevronLeft, ICON_SM, pal.text_muted))
                    .child(div().flex_1().child(BACK_LABEL))
                    .child(
                        div()
                            .text_size(px(TEXT_XS))
                            .text_color(rgb(pal.text_muted))
                            .child(OPEN_SETTINGS_HINT),
                    ),
            )
            .into_any_element()
    }

    fn settings_main(
        pane: SettingsPane,
        content: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        card(pal, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                WindowTitleBar::new(PAGE_BAR_ID)
                    .without_controls()
                    .padding(SPACE_3)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(SPACE_2))
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .child(div().text_color(rgb(pal.text_muted)).child(SETTINGS_TITLE))
                            .child(
                                div()
                                    .text_color(rgb(pal.text_muted))
                                    .child(BREADCRUMB_SEPARATOR),
                            )
                            .child(div().text_color(rgb(pal.text_strong)).child(pane.title())),
                    ),
            )
            .child(
                div()
                    .id(SETTINGS_SCROLL_ID)
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .overflow_y_scroll()
                    .px(px(SPACE_6))
                    .pb(px(SPACE_6))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .max_w(px(PAGE_W))
                            .pt(px(PAGE_TOP))
                            .gap(px(PAGE_GAP))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(SPACE_1))
                                    .child(
                                        div()
                                            .text_size(px(PAGE_TITLE_TEXT))
                                            .line_height(px(PAGE_TITLE_LINE))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(pal.text_strong))
                                            .child(pane.title()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(TEXT_SM))
                                            .line_height(px(LINE_SM))
                                            .text_color(rgb(pal.text_muted))
                                            .child(pane.description()),
                                    ),
                            )
                            .child(content),
                    ),
            )
            .into_any_element()
    }

    fn nav_row(pane: SettingsPane, selected: bool, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .id((NAV_ROW_PREFIX, pane as u64))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(NAV_ROW_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(if selected {
                pal.text_strong
            } else {
                pal.text_default
            }))
            .when(selected, |row| {
                row.bg(row_fill(pal, cx))
                    .shadow(row_shadow(pal, cx))
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!selected, |row| row.hover(|style| style.bg(pal.bg_hover)))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| this.select_pane(pane, cx)))
            .child(icon(pane.icon(), ICON_SM, pal.text_muted))
            .child(pane.title())
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_sits_in_one_group() {
        for pane in SettingsPane::ALL {
            let homes = GROUPS
                .iter()
                .filter(|(_, panes)| panes.contains(&pane))
                .count();
            assert_eq!(homes, 1, "{}", pane.title());
        }
    }
}
