mod rows;
mod view;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use krad_domain::{ExtensionEntry, IndexRepository};
use krad_extensions::index::{
    add_from_index, cached_index, load_index, update_from_index, Freshness, LoadedIndex,
};
use krad_extensions::ExtensionError;
use std::collections::BTreeSet;

use super::super::super::EnvironmentShell;
use super::super::SettingsPane;
use rows::LoadFailure;

const SEARCH_PLACEHOLDER: &str = "Search extensions";
const REPOSITORY_PLACEHOLDER: &str = "owner/name";
const NO_INDEX_TEXT: &str = "No extension index is loaded";
const UP_TO_DATE_TEXT: &str = "is already up to date";
const UPDATED_TEXT: &str = "was updated";
const INSTALL_FAILED_TEXT: &str = "could not be installed";

#[derive(Debug, Clone, Default, PartialEq)]
enum IndexState {
    #[default]
    NotLoaded,
    Loading,
    Loaded(LoadedIndex),
    Failed(LoadFailure),
}

#[derive(Default)]
pub(in crate::environment_shell) struct IndexBrowser {
    query: Option<Entity<InputState>>,
    repository: Option<Entity<InputState>>,
    state: IndexState,
    working: BTreeSet<String>,
    subscriptions: Vec<Subscription>,
}

impl IndexBrowser {
    fn loaded(&self) -> Option<&LoadedIndex> {
        match &self.state {
            IndexState::Loaded(loaded) => Some(loaded),
            _ => None,
        }
    }
}

enum PinnedUpdate {
    Moved(ExtensionEntry),
    Current,
}

fn failure_of(error: &ExtensionError) -> LoadFailure {
    if error.is_not_found() {
        LoadFailure::Missing
    } else {
        LoadFailure::Unreachable(error.to_string())
    }
}

impl EnvironmentShell {
    fn browser(&self) -> Option<&IndexBrowser> {
        self.settings_ui.as_ref().map(|ui| &ui.extensions.browse)
    }

    fn browser_mut(&mut self) -> Option<&mut IndexBrowser> {
        self.settings_ui
            .as_mut()
            .map(|ui| &mut ui.extensions.browse)
    }

    pub(in crate::environment_shell) fn sync_index_browser(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self
            .settings_ui
            .as_ref()
            .is_some_and(|ui| ui.pane == SettingsPane::Extensions);
        if !open {
            return;
        }
        let repository_text = self.settings.extension_index.to_string();
        let missing_inputs = self
            .browser()
            .is_some_and(|browser| browser.query.is_none() || browser.repository.is_none());
        if missing_inputs {
            let query = cx.new(|cx| InputState::new(window, cx).placeholder(SEARCH_PLACEHOLDER));
            let repository = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(repository_text)
                    .placeholder(REPOSITORY_PLACEHOLDER)
            });
            let searching = cx.subscribe_in(
                &query,
                window,
                |_: &mut Self, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                },
            );
            let choosing = cx.subscribe_in(
                &repository,
                window,
                |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.apply_index_repository(cx);
                    }
                },
            );
            if let Some(browser) = self.browser_mut() {
                browser.query = Some(query);
                browser.repository = Some(repository);
                browser.subscriptions.extend([searching, choosing]);
            }
        }
        let unloaded = self
            .browser()
            .is_some_and(|browser| browser.state == IndexState::NotLoaded);
        if unloaded {
            self.load_extension_index(Freshness::CachedIfFresh, cx);
        }
    }

    fn load_extension_index(&mut self, freshness: Freshness, cx: &mut Context<Self>) {
        let Some(config) = krad_store::config_dir() else {
            return;
        };
        let Some(browser) = self.browser_mut() else {
            return;
        };
        browser.state = IndexState::Loading;
        let repository = self.settings.extension_index.clone();
        let loading = cx.background_spawn(async move {
            load_index(&config, &repository, freshness).map_err(|error| failure_of(&error))
        });
        cx.spawn(async move |this, cx| {
            let loaded = loading.await;
            this.update(cx, |shell, cx| {
                if let Some(browser) = shell.browser_mut() {
                    browser.state = match loaded {
                        Ok(loaded) => IndexState::Loaded(loaded),
                        Err(failure) => IndexState::Failed(failure),
                    };
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    fn apply_index_repository(&mut self, cx: &mut Context<Self>) {
        let Some(field) = self
            .browser()
            .and_then(|browser| browser.repository.clone())
        else {
            return;
        };
        let text = field.read(cx).value().to_string();
        match text.parse::<IndexRepository>() {
            Ok(repository) if repository == self.settings.extension_index => {
                self.load_extension_index(Freshness::Refresh, cx);
            }
            Ok(repository) => {
                self.settings.extension_index = repository;
                self.save_settings(cx);
                self.load_extension_index(Freshness::Refresh, cx);
            }
            Err(error) => self.toast(&error.to_string(), cx),
        }
    }

    fn set_working(&mut self, id: &str, working: bool) {
        if let Some(browser) = self.browser_mut() {
            if working {
                browser.working.insert(id.to_owned());
            } else {
                browser.working.remove(id);
            }
        }
    }

    fn install_from_index(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(config) = krad_store::config_dir() else {
            return;
        };
        let Some(entry) = self
            .browser()
            .and_then(IndexBrowser::loaded)
            .and_then(|loaded| loaded.index.get(id))
            .cloned()
        else {
            return;
        };
        self.set_working(id, true);
        let installing = cx.background_spawn(async move {
            add_from_index(&config, &entry).map_err(|error| error.to_string())
        });
        let id = id.to_owned();
        cx.spawn(async move |this, cx| {
            let installed = installing.await;
            this.update(cx, |shell, cx| {
                shell.set_working(&id, false);
                match installed {
                    Ok(entry) => shell.add_extension(Ok(entry), cx),
                    Err(error) => shell.toast(&format!("{id} {INSTALL_FAILED_TEXT}: {error}"), cx),
                }
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    pub(in crate::environment_shell) fn update_pinned_extension(
        &mut self,
        entry: ExtensionEntry,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = krad_store::config_dir() else {
            return;
        };
        let known = self
            .browser()
            .and_then(IndexBrowser::loaded)
            .map(|loaded| loaded.index.clone());
        let repository = self.settings.extension_index.clone();
        let id = entry.id.clone();
        self.set_working(&id, true);
        let updating = cx.background_spawn(async move {
            let index = known
                .or_else(|| cached_index(&config, &repository).map(|loaded| loaded.index))
                .ok_or_else(|| NO_INDEX_TEXT.to_owned())?;
            match update_from_index(&config, &entry, &index) {
                Ok(Some(moved)) => Ok(PinnedUpdate::Moved(moved)),
                Ok(None) => Ok(PinnedUpdate::Current),
                Err(error) => Err(error.to_string()),
            }
        });
        cx.spawn(async move |this, cx| {
            let updated = updating.await;
            this.update(cx, |shell, cx| {
                shell.set_working(&id, false);
                match updated {
                    Ok(PinnedUpdate::Moved(moved)) => {
                        if let Some(slot) = shell
                            .settings
                            .extensions
                            .iter_mut()
                            .find(|known| known.id == moved.id)
                        {
                            *slot = moved;
                        }
                        shell.toast(&format!("{id} {UPDATED_TEXT}"), cx);
                        shell.extensions_changed(cx);
                    }
                    Ok(PinnedUpdate::Current) => {
                        shell.toast(&format!("{id} {UP_TO_DATE_TEXT}"), cx);
                    }
                    Err(error) => shell.toast(&error, cx),
                }
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }
}
