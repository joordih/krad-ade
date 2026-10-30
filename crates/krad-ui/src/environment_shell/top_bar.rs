use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, Action, AnyElement, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled,
};
use krad_domain::EnvironmentView;

use super::sidebar::SIDEBAR_TOGGLE_END;
use super::EnvironmentShell;
use crate::actions::ToggleSidePanel;
use crate::components::{icon, tooltip_view, tooltip_with_shortcut, IconName};
use crate::theme::{
    palette, Palette, Tint, ICON, ICON_BUTTON, ICON_SM, LINE_SM, SPACE_0_5, SPACE_1, SPACE_2,
    TEXT_SM,
};
use crate::title_bar::WindowTitleBar;

const TOP_BAR_ID: &str = "top-bar";
const BTN_SIDE_PANEL: &str = "side-panel";
const TOOLTIP_SIDE_PANEL: &str = "Files and terminals";
const BAR_PAD_X: f32 = SPACE_1;
const PANEL_BUTTON_RADIUS: f32 = 7.0;
const LIGHTS_GAP: f32 = SPACE_2;

impl EnvironmentShell {
    pub(super) fn top_bar(&self, cx: &mut Context<Self>) -> WindowTitleBar {
        WindowTitleBar::new(TOP_BAR_ID)
            .padding(BAR_PAD_X)
            .gap(LIGHTS_GAP)
            .leading(self.top_bar_leading())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(self.top_bar_middle(cx))
                    .children(self.app_update_pill(cx))
                    .child(self.panel_toggles(cx)),
            )
    }

    pub(super) fn top_bar_leading(&self) -> f32 {
        let (left, _) = self.content_origin();
        (SIDEBAR_TOGGLE_END - left).max(BAR_PAD_X)
    }

    fn top_bar_middle(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let view = self.active_view();
        let tabs = match view {
            EnvironmentView::Sessions => self.top_bar_tabs(cx),
            EnvironmentView::Notes => self.note_top_bar(cx),
            EnvironmentView::Review => None,
        };
        if let Some(tabs) = tabs {
            return tabs;
        }
        let session = (view == EnvironmentView::Sessions)
            .then(|| {
                self.envs
                    .get(&self.active)
                    .and_then(|live| live.data.current_session())
                    .map(|session| session.name.clone())
            })
            .flatten();
        let (glyph, view_label) = view_identity(view);
        let (glyph, label) = match session {
            Some(name) => (IconName::Layers, name),
            None => (glyph, view_label.to_owned()),
        };
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_2))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(pal.text_default))
            .child(icon(glyph, ICON_SM, pal.text_muted))
            .child(label)
            .into_any_element()
    }

    fn panel_toggles(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .flex()
            .flex_row()
            .flex_none()
            .gap(px(SPACE_0_5))
            .occlude()
            .child(
                panel_button(
                    BTN_SIDE_PANEL,
                    IconName::SidebarRight,
                    TOOLTIP_SIDE_PANEL,
                    Some(Box::new(ToggleSidePanel)),
                    self.right_sidebar_open().then_some(pal.bg_selected),
                    pal,
                )
                .on_click(cx.listener(|this: &mut Self, _, _, cx| this.toggle_side_panel(cx))),
            )
            .into_any_element()
    }

    pub(super) fn active_view(&self) -> EnvironmentView {
        self.envs
            .get(&self.active)
            .map_or(EnvironmentView::Sessions, |live| live.data.view)
    }
}

pub(super) fn offered_views(
    review_offered: bool,
) -> Vec<(EnvironmentView, IconName, &'static str)> {
    EnvironmentView::ALL
        .into_iter()
        .filter(|view| *view != EnvironmentView::Review || review_offered)
        .map(|view| {
            let (icon_name, label) = view_identity(view);
            (view, icon_name, label)
        })
        .collect()
}

pub(super) const fn view_identity(view: EnvironmentView) -> (IconName, &'static str) {
    match view {
        EnvironmentView::Sessions => (IconName::Mosaic, "Sessions"),
        EnvironmentView::Notes => (IconName::Book, "Notes"),
        EnvironmentView::Review => (IconName::Merge, "Review"),
    }
}

fn panel_button(
    id: &'static str,
    name: IconName,
    tooltip: &'static str,
    shortcut: Option<Box<dyn Action>>,
    fill: Option<Tint>,
    pal: &Palette,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(ICON_BUTTON))
        .rounded(px(PANEL_BUTTON_RADIUS))
        .cursor_pointer()
        .when_some(fill, Styled::bg)
        .when(fill.is_none(), |button| {
            button.hover(|style| style.bg(pal.bg_hover))
        })
        .tooltip(move |_, cx| match shortcut.as_deref() {
            Some(action) => tooltip_with_shortcut(tooltip.into(), action, cx),
            None => tooltip_view(tooltip.into(), cx),
        })
        .child(icon(name, ICON, pal.text_muted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn review_shows_only_when_offered() {
        let labels = |review| -> Vec<&str> {
            offered_views(review)
                .into_iter()
                .map(|(_, _, label)| label)
                .collect()
        };
        assert_eq!(labels(false), ["Sessions", "Notes"]);
        assert_eq!(labels(true), ["Sessions", "Notes", "Review"]);
    }

    #[test]
    fn every_view_has_its_own_label_and_icon() {
        let identities: Vec<(IconName, &str)> = EnvironmentView::ALL
            .into_iter()
            .map(view_identity)
            .collect();
        let labels: HashSet<&str> = identities.iter().map(|(_, label)| *label).collect();
        assert_eq!(labels.len(), EnvironmentView::ALL.len());
        let icons: Vec<IconName> = identities.iter().map(|(name, _)| *name).collect();
        for (index, name) in icons.iter().enumerate() {
            assert!(!icons[index + 1..].contains(name), "{name:?} is shared");
        }
    }
}
