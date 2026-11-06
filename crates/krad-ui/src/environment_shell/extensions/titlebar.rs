mod view;

use gpui_kit::{Context, FocusHandle, Window};
use krad_domain::EnvironmentName;
use krad_extensions::api::manifest::{
    ExtensionManifest, Placement, TitlebarAction, TitlebarItemContribution, TitlebarSide,
};
use krad_extensions::api::protocol::SetTitlebarItemParams;
use krad_extensions::api::{ExtensionId, Icon, QualifiedCommand, TitlebarItemId, ViewId};
use std::collections::HashMap;

use super::super::EnvironmentShell;
use super::views::ViewPlace;
use super::{ExtensionRecord, OpenViewId, SlotKey};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ButtonState {
    icon: Option<Icon>,
    title: Option<String>,
    active: bool,
    hidden: bool,
}

impl ButtonState {
    fn apply(&mut self, params: SetTitlebarItemParams) {
        if let Some(icon) = params.icon {
            self.icon = Some(icon);
        }
        if let Some(title) = params.title {
            self.title = Some(title);
        }
        if let Some(active) = params.active {
            self.active = active;
        }
        if let Some(hidden) = params.hidden {
            self.hidden = hidden;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ItemState {
    environment: Option<EnvironmentName>,
    item: TitlebarItemId,
    state: ButtonState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ShownButton<'a> {
    extension: &'a ExtensionId,
    item: &'a TitlebarItemContribution,
    icon: Icon,
    title: &'a str,
    active: bool,
}

pub(super) struct Popover {
    view: OpenViewId,
    focus: FocusHandle,
    focus_pending: bool,
}

#[derive(Default)]
pub(super) struct Titlebar {
    states: HashMap<ExtensionId, Vec<ItemState>>,
    popover: Option<Popover>,
}

impl Titlebar {
    fn state(
        &self,
        extension: &ExtensionId,
        environment: Option<&EnvironmentName>,
        item: &TitlebarItemId,
    ) -> Option<&ButtonState> {
        self.states
            .get(extension)?
            .iter()
            .find(|state| state.environment.as_ref() == environment && state.item == *item)
            .map(|state| &state.state)
    }

    fn apply(&mut self, key: &SlotKey, params: SetTitlebarItemParams) {
        let states = self.states.entry(key.extension.clone()).or_default();
        if let Some(found) = states
            .iter_mut()
            .find(|state| state.environment == key.environment && state.item == params.item)
        {
            found.state.apply(params);
            return;
        }
        let mut state = ButtonState::default();
        let item = params.item.clone();
        state.apply(params);
        states.push(ItemState {
            environment: key.environment.clone(),
            item,
            state,
        });
    }

    fn forget(&mut self, key: &SlotKey) {
        if let Some(states) = self.states.get_mut(&key.extension) {
            states.retain(|state| state.environment != key.environment);
        }
    }

    fn shown<'a>(
        &'a self,
        manifest: &'a ExtensionManifest,
        environment: Option<&'a EnvironmentName>,
    ) -> impl Iterator<Item = ShownButton<'a>> + 'a {
        manifest
            .contributes
            .titlebar_items
            .iter()
            .filter_map(move |item| {
                let state = self.state(&manifest.id, environment, &item.id);
                if state.is_some_and(|state| state.hidden) {
                    return None;
                }
                Some(ShownButton {
                    extension: &manifest.id,
                    item,
                    icon: state.and_then(|state| state.icon).unwrap_or(item.icon),
                    title: state
                        .and_then(|state| state.title.as_deref())
                        .unwrap_or(&item.title),
                    active: state.is_some_and(|state| state.active),
                })
            })
    }
}

fn environment_of<'a>(
    manifest: &ExtensionManifest,
    active: &'a EnvironmentName,
) -> Option<&'a EnvironmentName> {
    match manifest.program.as_ref().map(|program| program.placement) {
        Some(Placement::Environment) => Some(active),
        Some(Placement::App) | None => None,
    }
}

fn opens_view(item: &TitlebarItemContribution, view: &ViewId) -> bool {
    matches!(&item.action, TitlebarAction::View(opened) if opened == view)
}

impl EnvironmentShell {
    pub(super) fn titlebar_buttons(&self, side: TitlebarSide) -> Vec<ShownButton<'_>> {
        self.extensions
            .records
            .iter()
            .filter(|record| record.usable())
            .filter_map(ExtensionRecord::manifest)
            .flat_map(|manifest| {
                self.extensions
                    .titlebar
                    .shown(manifest, environment_of(manifest, &self.active))
            })
            .filter(|button| button.item.side == side)
            .collect()
    }

    pub(super) fn popover_anchor(
        &self,
        manifest: &ExtensionManifest,
        view: &ViewId,
    ) -> Option<TitlebarItemId> {
        self.extensions
            .titlebar
            .shown(manifest, environment_of(manifest, &self.active))
            .find(|button| opens_view(button.item, view))
            .map(|button| button.item.id.clone())
    }

    pub(super) fn set_titlebar_item(
        &mut self,
        key: &SlotKey,
        params: SetTitlebarItemParams,
        cx: &mut Context<Self>,
    ) {
        let declared = self
            .extensions
            .record(&key.extension)
            .and_then(ExtensionRecord::manifest)
            .is_some_and(|manifest| {
                manifest
                    .contributes
                    .titlebar_items
                    .iter()
                    .any(|item| item.id == params.item)
            });
        if !declared {
            tracing::debug!(extension = %key.extension, "titlebar item not declared");
            return;
        }
        let item = params.item.clone();
        let hides = params.hidden == Some(true);
        self.extensions.titlebar.apply(key, params.bounded());
        if hides && self.popover_of(&key.extension, &item).is_some() {
            self.close_extension_popover(cx);
        }
        cx.notify();
    }

    pub(super) fn forget_titlebar_items(&mut self, key: &SlotKey) {
        self.extensions.titlebar.forget(key);
    }

    pub(super) fn popover_of(
        &self,
        extension: &ExtensionId,
        item: &TitlebarItemId,
    ) -> Option<OpenViewId> {
        let id = self.extensions.titlebar.popover.as_ref()?.view;
        let open = self.extensions.views.get(id)?;
        match &open.place {
            ViewPlace::Popover(_, anchor) if open.key.extension == *extension && anchor == item => {
                Some(id)
            }
            _ => None,
        }
    }

    pub(super) fn show_extension_popover(&mut self, id: OpenViewId, cx: &mut Context<Self>) {
        if self
            .extensions
            .titlebar
            .popover
            .as_ref()
            .is_some_and(|popover| popover.view == id)
        {
            return;
        }
        self.close_extension_popover(cx);
        self.extensions.titlebar.popover = Some(Popover {
            view: id,
            focus: cx.focus_handle(),
            focus_pending: true,
        });
    }

    pub(super) fn forget_extension_popover(&mut self, id: OpenViewId) {
        if self
            .extensions
            .titlebar
            .popover
            .as_ref()
            .is_some_and(|popover| popover.view == id)
        {
            self.extensions.titlebar.popover = None;
        }
    }

    pub(in crate::environment_shell) fn close_extension_popover(&mut self, cx: &mut Context<Self>) {
        if let Some(popover) = self.extensions.titlebar.popover.take() {
            self.close_extension_view(popover.view, cx);
        }
    }

    pub(in crate::environment_shell) fn sync_extension_popover_focus(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(popover) = self
            .extensions
            .titlebar
            .popover
            .as_mut()
            .filter(|popover| popover.focus_pending)
        {
            popover.focus_pending = false;
            window.focus(&popover.focus, cx);
        }
    }

    fn press_titlebar_item(
        &mut self,
        extension: &ExtensionId,
        item: &TitlebarItemId,
        cx: &mut Context<Self>,
    ) {
        if self.popover_of(extension, item).is_some() {
            self.close_extension_popover(cx);
            return;
        }
        let Some(action) = self
            .extensions
            .record(extension)
            .filter(|record| record.usable())
            .and_then(ExtensionRecord::manifest)
            .and_then(|manifest| {
                manifest
                    .contributes
                    .titlebar_items
                    .iter()
                    .find(|declared| declared.id == *item)
            })
            .map(|declared| declared.action.clone())
        else {
            return;
        };
        self.close_extension_popover(cx);
        match action {
            TitlebarAction::View(view) => {
                self.open_extension_view(extension, &view, None, None, cx);
            }
            TitlebarAction::Command(command) => self.run_extension_command(
                QualifiedCommand {
                    extension: extension.clone(),
                    command,
                },
                None,
                cx,
            ),
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
