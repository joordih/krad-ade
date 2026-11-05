use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled,
};
use krad_domain::{EnvironmentName, ExtensionTab, ShownExtensionViews, TerminalId};
use krad_extensions::api::manifest::ViewZone;

use super::super::super::activity::UtilityTab;
use super::super::super::session_pane::{tab_shell, TabWidth};
use super::super::super::EnvironmentShell;
use super::super::commands::icon_of;
use super::{OpenViewId, ViewPlace, ViewState};
use crate::components::icon;
use crate::theme::{
    palette, row_fill, row_shadow, ICON_SM, LINE_SM, NAV_ROW_H, RADIUS_XS, SPACE_2, SPACE_3,
    TEXT_SM,
};

const LOADING_TEXT: &str = "Loading…";
const SIDEBAR_ROW_PREFIX: &str = "sidebar-extension-view";
const SIDE_TAB_PREFIX: &str = "side-extension-view";
const UTILITY_TAB_PREFIX: &str = "utilities-extension-view";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn reveal_extension_view(
        &mut self,
        id: OpenViewId,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let Some(place) = self.extensions.views.get(id).map(|open| open.place.clone()) else {
            return;
        };
        match place {
            ViewPlace::Sidebar(env) => {
                if let Some(live) = self.envs.get_mut(&env) {
                    live.extension_sidebar = Some(id);
                }
                self.remember_extension_views(&env);
            }
            ViewPlace::SidePanel(env) => {
                if let Some(live) = self.envs.get_mut(&env) {
                    live.extension_side = Some(id);
                    live.data.side_panel.open = true;
                }
                self.remember_extension_views(&env);
                self.store();
            }
            ViewPlace::Utilities(_) => self.show_utilities(UtilityTab::Extension(id), cx),
            ViewPlace::FileTab(env, terminal) => {
                if let Some(live) = self.envs.get_mut(&env) {
                    live.data.reveal_terminal(terminal);
                    live.focused = Some(terminal);
                }
            }
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn extension_view_body(
        &self,
        id: OpenViewId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let message = |text: String| {
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .p(px(SPACE_3))
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(pal.text_muted))
                .child(text)
                .into_any_element()
        };
        match self.extensions.views.get(id).map(|open| &open.state) {
            Some(ViewState::Shown { entity, .. }) => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .size_full()
                .child(entity.clone())
                .into_any_element(),
            Some(ViewState::Failed(error)) => message(error.clone()),
            Some(ViewState::Loading) | None => message(LOADING_TEXT.to_owned()),
        }
    }

    pub(in crate::environment_shell) fn extension_main_view(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.editing() {
            return None;
        }
        let id = self.envs.get(&self.active)?.extension_sidebar?;
        Some(self.extension_view_body(id, cx))
    }

    pub(in crate::environment_shell) fn hide_extension_main_view(&mut self) {
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            live.extension_sidebar = None;
        }
        self.remember_extension_views(&env);
    }

    fn shown_title(&self, id: Option<OpenViewId>) -> Option<(String, String)> {
        let open = self.extensions.views.get(id?)?;
        Some((open.key.extension.to_string(), open.view.view.to_string()))
    }

    pub(in crate::environment_shell) fn extension_sidebar_rows(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pal = palette(cx);
        let shown = self
            .envs
            .get(&self.active)
            .and_then(|live| live.extension_sidebar);
        let current = self.shown_title(shown);
        self.declared_views()
            .into_iter()
            .filter(|(_, view)| view.zone == ViewZone::Sidebar)
            .enumerate()
            .map(|(index, (extension, view))| {
                let selected = current.as_ref().is_some_and(|(owner, shown_view)| {
                    *owner == extension.to_string() && *shown_view == view.id.to_string()
                });
                let opened = (extension.clone(), view.id.clone());
                div()
                    .id(ElementId::from((SIDEBAR_ROW_PREFIX, index)))
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
                        this.open_extension_view(&opened.0, &opened.1, None, None, cx);
                    }))
                    .child(icon(icon_of(view.icon), ICON_SM, pal.text_default))
                    .child(view.title.clone())
                    .into_any_element()
            })
            .collect()
    }

    pub(in crate::environment_shell) fn extension_side_tabs(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pal = palette(cx);
        let shown = self
            .envs
            .get(&self.active)
            .and_then(|live| live.extension_side);
        let current = self.shown_title(shown);
        self.declared_views()
            .into_iter()
            .filter(|(_, view)| view.zone == ViewZone::SidePanel)
            .enumerate()
            .map(|(index, (extension, view))| {
                let active = current.as_ref().is_some_and(|(owner, shown_view)| {
                    *owner == extension.to_string() && *shown_view == view.id.to_string()
                });
                let opened = (extension.clone(), view.id.clone());
                tab_shell((SIDE_TAB_PREFIX, index), active, TabWidth::Fit, pal, cx)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.open_extension_view(&opened.0, &opened.1, None, None, cx);
                    }))
                    .child(icon(icon_of(view.icon), ICON_SM, pal.text_muted))
                    .child(view.title.clone())
                    .into_any_element()
            })
            .collect()
    }

    pub(in crate::environment_shell) fn extension_side_body(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = self.envs.get(&self.active)?.extension_side?;
        Some(self.extension_view_body(id, cx))
    }

    pub(in crate::environment_shell) fn hide_extension_side_view(&mut self) {
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            live.extension_side = None;
        }
        self.remember_extension_views(&env);
    }

    fn shown_tab(&self, id: Option<OpenViewId>) -> Option<ExtensionTab> {
        let open = self.extensions.views.get(id?)?;
        Some(ExtensionTab {
            extension: open.key.extension.to_string(),
            view: open.view.view.to_string(),
            instance: open.view.instance.as_ref().map(ToString::to_string),
        })
    }

    pub(in crate::environment_shell) fn remember_extension_views(&mut self, env: &EnvironmentName) {
        let Some(live) = self.envs.get(env) else {
            return;
        };
        let shown = ShownExtensionViews {
            sidebar: self.shown_tab(live.extension_sidebar),
            side_panel: self.shown_tab(live.extension_side),
        };
        if live.data.extension_views == shown {
            return;
        }
        if let Some(live) = self.envs.get_mut(env) {
            live.data.extension_views = shown;
        }
        self.store();
    }

    pub(in crate::environment_shell) fn restore_extension_views(&mut self, cx: &mut Context<Self>) {
        let env = self.active.clone();
        if self.extensions.records.is_empty() || self.extensions.restored.contains(&env) {
            return;
        }
        self.extensions.restored.insert(env.clone());
        let Some(live) = self.envs.get(&env) else {
            return;
        };
        let wanted = live.data.extension_views.clone();
        let missing: Vec<ExtensionTab> = [
            (wanted.sidebar, live.extension_sidebar),
            (wanted.side_panel, live.extension_side),
        ]
        .into_iter()
        .filter(|(_, shown)| shown.is_none())
        .filter_map(|(tab, _)| tab)
        .collect();
        for tab in missing {
            let (Ok(extension), Ok(view)) = (tab.extension.parse(), tab.view.parse()) else {
                continue;
            };
            let instance = tab.instance.and_then(|instance| instance.parse().ok());
            self.open_extension_view(&extension, &view, instance, None, cx);
        }
    }

    pub(in crate::environment_shell) fn extension_utility_tabs(
        &self,
        shown: UtilityTab,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pal = palette(cx);
        let active = self.active.clone();
        let mut open: Vec<(OpenViewId, String)> = self
            .extensions
            .views
            .views
            .iter()
            .filter(|(_, open)| open.place == ViewPlace::Utilities(active.clone()))
            .map(|(id, open)| (*id, open.title.clone()))
            .collect();
        open.sort_by_key(|(id, _)| id.0);
        open.into_iter()
            .map(|(id, title)| {
                tab_shell(
                    (UTILITY_TAB_PREFIX, id.0),
                    shown == UtilityTab::Extension(id),
                    TabWidth::Fit,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.set_utility_tab(UtilityTab::Extension(id), cx);
                }))
                .child(icon(
                    crate::components::IconName::Plug,
                    ICON_SM,
                    pal.text_muted,
                ))
                .child(div().font_weight(FontWeight::MEDIUM).child(title))
                .into_any_element()
            })
            .collect()
    }

    pub(in crate::environment_shell) fn extension_tab_body(
        &self,
        terminal: TerminalId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.active.clone();
        let id = self
            .extensions
            .views
            .views
            .iter()
            .find(|(_, open)| open.place == ViewPlace::FileTab(active.clone(), terminal))
            .map(|(id, _)| *id);
        match id {
            Some(id) => self.extension_view_body(id, cx),
            None => div().into_any_element(),
        }
    }

    pub(in crate::environment_shell) fn extension_view_open(&self, id: OpenViewId) -> bool {
        self.extensions.views.get(id).is_some()
    }
}
