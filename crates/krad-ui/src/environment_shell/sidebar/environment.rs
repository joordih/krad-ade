use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use krad_domain::{EnvironmentView, SidebarDensity, SidebarLayout, SidebarPart};

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::top_bar::view_identity;
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
        sidebar_options(&self.settings.sidebar)
    }

    pub(in crate::environment_shell) fn run_sidebar_option(
        &mut self,
        action: &MenuAction,
        cx: &mut Context<Self>,
    ) {
        let layout = &mut self.settings.sidebar;
        match action {
            MenuAction::ToggleSidebarPart(part) => layout.toggle(*part),
            MenuAction::ToggleSidebarDensity => layout.density = layout.density.toggled(),
            MenuAction::ToggleSidebarView(view) => {
                layout.toggle_view(*view);
                self.leave_hidden_views();
            }
            _ => return,
        }
        self.save_settings(cx);
        let entries = self.sidebar_options();
        self.replace_menu_entries(entries);
        cx.notify();
    }

    fn leave_hidden_views(&mut self) {
        let layout = &self.settings.sidebar;
        let mut moved = false;
        for live in self.envs.values_mut() {
            let reachable = layout.reachable_view(live.data.view);
            moved |= reachable != live.data.view;
            live.data.view = reachable;
        }
        if moved {
            self.store();
        }
    }
}

const fn part_icon(part: SidebarPart) -> IconName {
    match part {
        SidebarPart::Views => IconName::ListView,
        SidebarPart::Branches => IconName::GitBranch,
        SidebarPart::AgentCounts => IconName::UserGroup,
    }
}

fn sidebar_options(layout: &SidebarLayout) -> Vec<MenuEntry> {
    let views = EnvironmentView::ALL
        .into_iter()
        .filter(|view| view.is_optional())
        .map(|view| {
            let (glyph, label) = view_identity(view);
            MenuEntry::switch(
                label,
                glyph,
                layout.shows_view(view),
                MenuAction::ToggleSidebarView(view),
            )
        });
    let parts = SidebarPart::ALL.into_iter().map(|part| {
        MenuEntry::switch(
            part.display_name(),
            part_icon(part),
            layout.shows(part),
            MenuAction::ToggleSidebarPart(part),
        )
    });
    let compact = MenuEntry::switch(
        COMPACT_LABEL,
        IconName::CollapseAll,
        layout.density == SidebarDensity::Compact,
        MenuAction::ToggleSidebarDensity,
    );
    views
        .chain(std::iter::once(MenuEntry::Separator))
        .chain(parts)
        .chain(std::iter::once(compact))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn switches(entries: &[MenuEntry]) -> Vec<(&str, bool)> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Item { label, checked, .. } if entry.is_switch() => {
                    Some((label.as_str(), *checked))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_menu_lists_optional_views_then_the_sidebar_parts_as_switches() {
        let mut layout = SidebarLayout::default();
        layout.toggle_view(EnvironmentView::Kraddies);
        layout.density = SidebarDensity::Compact;
        let entries = sidebar_options(&layout);
        assert_eq!(
            switches(&entries),
            [
                ("Notes", true),
                ("Automations", true),
                ("Kraddies", false),
                ("Review", true),
                ("Views", true),
                ("Branches", true),
                ("Agent counts", true),
                (COMPACT_LABEL, true),
            ]
        );
        assert!(matches!(entries.get(4), Some(MenuEntry::Separator)));
        assert!(entries
            .iter()
            .all(|entry| matches!(entry, MenuEntry::Separator) || entry.is_switch()));
    }
}
