use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use krad_domain::{SidebarDensity, SidebarPart};

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::EnvironmentShell;
use crate::components::{icon, Button, IconName};
use crate::theme::{
    palette, ICON_SM, ICON_XS, LINE_BASE, RADIUS_SM, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2,
    TEXT_BASE,
};

const BTN_ENVIRONMENT: &str = "sidebar-environment";
const BTN_SIDEBAR_OPTIONS: &str = "sidebar-options";
const SIDEBAR_OPTIONS_TOOLTIP: &str = "Choose what the sidebar shows";
const COMPACT_LABEL: &str = "Compact view";
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
        let options = Button::new(BTN_SIDEBAR_OPTIONS)
            .icon(IconName::ListView)
            .small()
            .tooltip(SIDEBAR_OPTIONS_TOOLTIP)
            .on_click(
                cx.listener(|this: &mut Self, event: &gpui_kit::ClickEvent, _, cx| {
                    let at = event.position();
                    let entries = this.sidebar_options();
                    this.open_menu(
                        (at.x.as_f32(), at.y.as_f32()),
                        MenuPlacement::Below,
                        entries,
                        cx,
                    );
                }),
            );
        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(SPACE_1_5))
            .pb(px(SPACE_2))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1))
                    .child(div().flex_1().min_w_0().child(pill))
                    .child(options),
            )
            .children(self.host_badge(cx))
            .into_any_element()
    }

    fn sidebar_options(&self) -> Vec<MenuEntry> {
        let layout = &self.settings.sidebar;
        let mut entries: Vec<MenuEntry> = SidebarPart::ALL
            .into_iter()
            .map(|part| {
                MenuEntry::checked(
                    part.display_name(),
                    layout.shows(part),
                    MenuAction::ToggleSidebarPart(part),
                )
            })
            .collect();
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::checked(
            COMPACT_LABEL,
            layout.density == SidebarDensity::Compact,
            MenuAction::ToggleSidebarDensity,
        ));
        entries
    }

    pub(in crate::environment_shell) fn toggle_sidebar_part(
        &mut self,
        part: SidebarPart,
        cx: &mut Context<Self>,
    ) {
        self.settings.sidebar.toggle(part);
        self.save_settings(cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn toggle_sidebar_density(&mut self, cx: &mut Context<Self>) {
        self.settings.sidebar.density = self.settings.sidebar.density.toggled();
        self.save_settings(cx);
        cx.notify();
    }
}
