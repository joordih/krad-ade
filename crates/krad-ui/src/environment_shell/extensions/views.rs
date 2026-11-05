mod tabs;
mod terminal_link;
mod zones;

use futures::channel::mpsc;
use futures::StreamExt as _;
use gpui_kit::{AppContext as _, Context, Entity, Task};
use krad_domain::{EnvironmentName, ExtensionTab, TerminalId};
use krad_extensions::api::manifest::ViewZone;
use krad_extensions::api::protocol::{
    PatchViewParams, RenderView, Rows, RowsParams, ViewEventNotification, ViewEventParams, ViewRef,
};
use krad_extensions::api::{ExtensionId, ViewId, ViewInstance};
use std::collections::HashMap;

use super::super::EnvironmentShell;
use super::{Deferred, SlotKey};
use crate::extension_view::{ExtensionView, ViewOutput};

const VIEW_FAILED_TEXT: &str = "The extension stopped. Retry it from Settings.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::environment_shell) struct OpenViewId(u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ViewPlace {
    Sidebar(EnvironmentName),
    SidePanel(EnvironmentName),
    Utilities(EnvironmentName),
    FileTab(EnvironmentName, TerminalId),
    Popover(EnvironmentName, krad_extensions::api::TitlebarItemId),
}

pub(super) enum ViewState {
    Loading,
    Shown {
        entity: Entity<ExtensionView>,
        _output: Task<()>,
    },
    Failed(String),
}

pub(super) struct OpenView {
    pub(super) key: SlotKey,
    pub(super) view: ViewRef,
    pub(super) title: String,
    pub(super) place: ViewPlace,
    pub(super) state: ViewState,
}

#[derive(Default)]
pub(super) struct OpenViews {
    views: HashMap<OpenViewId, OpenView>,
    next: u64,
}

impl OpenViews {
    fn find(
        &self,
        key: &SlotKey,
        view: &ViewRef,
        place_env: &EnvironmentName,
    ) -> Option<OpenViewId> {
        self.views
            .iter()
            .find(|(_, open)| {
                open.key == *key && open.view == *view && place_env_of(&open.place) == place_env
            })
            .map(|(id, _)| *id)
    }

    pub(super) fn get(&self, id: OpenViewId) -> Option<&OpenView> {
        self.views.get(&id)
    }
}

const fn place_env_of(place: &ViewPlace) -> &EnvironmentName {
    match place {
        ViewPlace::Sidebar(env)
        | ViewPlace::SidePanel(env)
        | ViewPlace::Utilities(env)
        | ViewPlace::FileTab(env, _)
        | ViewPlace::Popover(env, _) => env,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_extension_view(
        &mut self,
        extension: &ExtensionId,
        view: &ViewId,
        instance: Option<ViewInstance>,
        title: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(manifest) = self
            .extensions
            .record(extension)
            .filter(|record| record.usable())
            .and_then(super::ExtensionRecord::manifest)
            .cloned()
        else {
            return false;
        };
        let Some(declared) = manifest
            .contributes
            .views
            .iter()
            .find(|declared| declared.id == *view)
        else {
            return false;
        };
        let Some(key) = self.slot_key(&manifest) else {
            return false;
        };
        let reference = ViewRef {
            view: view.clone(),
            instance: instance.clone(),
        };
        let env = self.active.clone();
        let title = title.unwrap_or_else(|| declared.title.clone());
        if let Some(id) = self.extensions.views.find(&key, &reference, &env) {
            self.reveal_extension_view(id, cx);
            return true;
        }
        let place = match declared.zone {
            ViewZone::Sidebar => ViewPlace::Sidebar(env),
            ViewZone::SidePanel => ViewPlace::SidePanel(env),
            ViewZone::Utilities => ViewPlace::Utilities(env),
            ViewZone::Popover => {
                let Some(item) = self.popover_anchor(&manifest, view) else {
                    return false;
                };
                ViewPlace::Popover(env, item)
            }
            ViewZone::FileTab => {
                let Some(live) = self.envs.get_mut(&env) else {
                    return false;
                };
                let group = live.selected_group;
                let tab = ExtensionTab {
                    extension: extension.to_string(),
                    view: view.to_string(),
                    instance: instance.map(String::from),
                };
                let terminal = live.data.open_extension_tab(group, tab, &title);
                live.focused = Some(terminal);
                self.store();
                ViewPlace::FileTab(env, terminal)
            }
        };
        let id = self.insert_view(key.clone(), reference, title, place);
        self.reveal_extension_view(id, cx);
        self.ensure_running(key, Deferred::Render(id), cx);
        true
    }

    fn insert_view(
        &mut self,
        key: SlotKey,
        view: ViewRef,
        title: String,
        place: ViewPlace,
    ) -> OpenViewId {
        let views = &mut self.extensions.views;
        views.next += 1;
        let id = OpenViewId(views.next);
        views.views.insert(
            id,
            OpenView {
                key,
                view,
                title,
                place,
                state: ViewState::Loading,
            },
        );
        id
    }

    pub(super) fn render_view(&mut self, id: OpenViewId, cx: &mut Context<Self>) {
        let Some(open) = self.extensions.views.get(id) else {
            return;
        };
        let Some(connection) = self.extensions.connection(&open.key) else {
            return;
        };
        let reference = open.view.clone();
        cx.spawn(async move |this, cx| {
            let rendered = connection.request::<RenderView>(&reference).await;
            this.update(cx, |shell, cx| match rendered {
                Ok(root) => shell.show_rendered(id, root, cx),
                Err(error) => {
                    if let Some(open) = shell.extensions.views.views.get_mut(&id) {
                        open.state = ViewState::Failed(error.to_string());
                    }
                    cx.notify();
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn show_rendered(
        &mut self,
        id: OpenViewId,
        mut root: krad_extensions::api::view::Node,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = krad_extensions::api::view::bounded(&mut root) {
            if let Some(open) = self.extensions.views.views.get_mut(&id) {
                open.state = ViewState::Failed(error.to_string());
            }
            cx.notify();
            return;
        }
        let Some(open) = self.extensions.views.views.get(&id) else {
            return;
        };
        if let ViewState::Shown { entity, .. } = &open.state {
            entity.update(cx, |view, cx| view.replace_root(root, cx));
            return;
        }
        let Some(connection) = self.extensions.connection(&open.key) else {
            return;
        };
        let reference = open.view.clone();
        let (output, mut outputs) = mpsc::unbounded();
        let folder = self
            .extensions
            .record(&open.key.extension)
            .and_then(super::ExtensionRecord::program)
            .map(|program| program.folder().to_path_buf());
        let entity = cx.new(|_| {
            let view = ExtensionView::new(root, output);
            match folder {
                Some(folder) => view.with_folder(folder),
                None => view,
            }
        });
        let shown = entity.downgrade();
        let task = cx.spawn(async move |_, cx| {
            while let Some(output) = outputs.next().await {
                match output {
                    ViewOutput::Event(event) => {
                        let params = ViewEventParams {
                            view: reference.clone(),
                            event,
                        };
                        if let Err(error) = connection.notify::<ViewEventNotification>(&params) {
                            tracing::debug!(%error, "view event not delivered");
                        }
                    }
                    ViewOutput::Rows { node, start, end } => {
                        let params = RowsParams {
                            view: reference.clone(),
                            node: node.clone(),
                            start,
                            end,
                        };
                        let bounded = RowsParams {
                            end: params.bounded_end(),
                            ..params
                        };
                        match connection.request::<Rows>(&bounded).await {
                            Ok(rows) => {
                                let applied =
                                    shown.update(cx, |view, cx| view.set_rows(node, rows, cx));
                                if applied.is_err() {
                                    break;
                                }
                            }
                            Err(error) => tracing::debug!(%error, "view rows not answered"),
                        }
                    }
                }
            }
        });
        let key = open.key.clone();
        for (owner, handle, link) in &self.extensions.view_terminals {
            if *owner == key {
                let link = link.clone();
                entity.update(cx, |view, cx| view.set_terminal(*handle, link, cx));
            }
        }
        if let Some(open) = self.extensions.views.views.get_mut(&id) {
            open.state = ViewState::Shown {
                entity,
                _output: task,
            };
        }
        cx.notify();
    }

    pub(super) fn patch_extension_views(
        &mut self,
        key: &SlotKey,
        params: &PatchViewParams,
        cx: &mut Context<Self>,
    ) {
        let targets: Vec<Entity<ExtensionView>> = self
            .extensions
            .views
            .views
            .values()
            .filter(|open| open.key == *key && open.view == params.view)
            .filter_map(|open| match &open.state {
                ViewState::Shown { entity, .. } => Some(entity.clone()),
                ViewState::Loading | ViewState::Failed(_) => None,
            })
            .collect();
        for entity in targets {
            let patches = params.patches.clone();
            entity.update(cx, |view, cx| view.apply(patches, cx));
        }
    }

    pub(super) fn render_views_of(&mut self, key: &SlotKey, cx: &mut Context<Self>) {
        let ids: Vec<OpenViewId> = self
            .extensions
            .views
            .views
            .iter()
            .filter(|(_, open)| open.key == *key)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.render_view(id, cx);
        }
    }

    pub(super) fn fail_views_of(&mut self, key: &SlotKey, cx: &mut Context<Self>) {
        for open in self.extensions.views.views.values_mut() {
            if open.key == *key {
                open.state = ViewState::Failed(VIEW_FAILED_TEXT.to_owned());
            }
        }
        cx.notify();
    }

    pub(super) fn drop_views_of(&mut self, key: &SlotKey, cx: &mut Context<Self>) {
        let ids: Vec<OpenViewId> = self
            .extensions
            .views
            .views
            .iter()
            .filter(|(_, open)| open.key == *key)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.close_extension_view(id, cx);
        }
    }
}
