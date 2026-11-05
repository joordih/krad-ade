use gpui_kit::Context;
use krad_domain::{EnvironmentName, ExtensionTab, NodeKind, TerminalId};
use krad_extensions::api::protocol::{ViewClosed, ViewRef};
use krad_extensions::api::{ExtensionId, ViewId};

use super::super::super::activity::UtilityTab;
use super::super::super::EnvironmentShell;
use super::super::{Deferred, SlotKey};
use super::{OpenViewId, ViewPlace};

impl EnvironmentShell {
    pub(in crate::environment_shell::extensions) fn close_extension_views(
        &mut self,
        key: &SlotKey,
        view: &ViewRef,
        cx: &mut Context<Self>,
    ) {
        let ids: Vec<OpenViewId> = self
            .extensions
            .views
            .views
            .iter()
            .filter(|(_, open)| open.key == *key && open.view == *view)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.close_extension_view(id, cx);
        }
    }

    pub(in crate::environment_shell) fn close_extension_view(
        &mut self,
        id: OpenViewId,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.extensions.views.views.remove(&id) else {
            return;
        };
        if let Some(connection) = self.extensions.connection(&open.key) {
            if let Err(error) = connection.notify::<ViewClosed>(&open.view) {
                tracing::debug!(%error, "view close not delivered");
            }
        }
        match open.place {
            ViewPlace::Sidebar(env) => {
                if let Some(live) = self.envs.get_mut(&env) {
                    live.extension_sidebar = live.extension_sidebar.filter(|shown| *shown != id);
                }
                self.remember_extension_views(&env);
            }
            ViewPlace::SidePanel(env) => {
                if let Some(live) = self.envs.get_mut(&env) {
                    live.extension_side = live.extension_side.filter(|shown| *shown != id);
                }
                self.remember_extension_views(&env);
            }
            ViewPlace::Utilities(env) => {
                if let Some(live) = self.envs.get_mut(&env) {
                    if live.utility_tab == UtilityTab::Extension(id) {
                        live.utility_tab = UtilityTab::Log;
                    }
                }
            }
            ViewPlace::FileTab(env, terminal) => {
                if let Some(live) = self.envs.get_mut(&env) {
                    if live.data.nodes.iter().any(|node| node.id == terminal) {
                        live.data.nodes.retain(|node| node.id != terminal);
                        live.data.remove_terminal_tiles(terminal);
                    }
                }
                self.store();
            }
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn sync_extension_tabs(&mut self, cx: &mut Context<Self>) {
        let closed: Vec<OpenViewId> =
            self.extensions
                .views
                .views
                .iter()
                .filter(|(_, open)| match &open.place {
                    ViewPlace::FileTab(env, terminal) => !self.envs.get(env).is_some_and(|live| {
                        live.data.nodes.iter().any(|node| node.id == *terminal)
                    }),
                    _ => false,
                })
                .map(|(id, _)| *id)
                .collect();
        for id in closed {
            self.close_extension_view(id, cx);
        }
        if self.extensions.records.is_empty() {
            return;
        }
        let env = self.active.clone();
        let restored: Vec<(TerminalId, ExtensionTab, String)> =
            self.envs.get(&env).map_or_else(Vec::new, |live| {
                live.data
                    .nodes
                    .iter()
                    .filter(|node| node.kind == NodeKind::Extension)
                    .filter_map(|node| Some((node.id, node.extension.clone()?, node.title.clone())))
                    .collect()
            });
        for (terminal, tab, title) in restored {
            let known = self
                .extensions
                .views
                .views
                .values()
                .any(|open| open.place == ViewPlace::FileTab(env.clone(), terminal));
            if !known {
                self.adopt_extension_tab(&env, terminal, &tab, title, cx);
            }
        }
    }

    fn adopt_extension_tab(
        &mut self,
        env: &EnvironmentName,
        terminal: TerminalId,
        tab: &ExtensionTab,
        title: String,
        cx: &mut Context<Self>,
    ) {
        let (Ok(extension), Ok(view)) = (
            tab.extension.parse::<ExtensionId>(),
            tab.view.parse::<ViewId>(),
        ) else {
            return;
        };
        let Some(manifest) = self
            .extensions
            .record(&extension)
            .filter(|record| record.usable())
            .and_then(super::super::ExtensionRecord::manifest)
            .cloned()
        else {
            return;
        };
        let Some(key) = self.slot_key(&manifest) else {
            return;
        };
        let instance = tab
            .instance
            .as_deref()
            .and_then(|instance| instance.parse().ok());
        let reference = ViewRef { view, instance };
        let id = self.insert_view(
            key.clone(),
            reference,
            title,
            ViewPlace::FileTab(env.clone(), terminal),
        );
        self.ensure_running(key, Deferred::Render(id), cx);
    }
}
