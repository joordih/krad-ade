use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement,
    PathPromptOptions, SharedString, Styled, Window,
};
use krad_domain::{ExtensionEntry, ExtensionSource, ServerRuntimeId};
use krad_extensions::{
    add_folder, add_git, remove, update, user_manifests_dir, ServerRuntimeStatus,
};

use super::{form_row, hint, panel_card, EnvironmentShell};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, SPACE_2, SPACE_3};

const SERVER_RUNTIMES_HINT: &str = "The runtimes language servers run on, such as Node.js. The app looks in the folder you choose, then on your system, then in the copy it installed. The toolchain each project is analysed with is chosen from the editor's status bar.";
const EXTENSIONS_HINT: &str =
    "An extension is a folder of language and server runtime manifests. Your own manifests go in";
const NOT_FOUND_TEXT: &str = "Not found";
const CHOSEN_TEXT: &str = "chosen";
const URL_PLACEHOLDER: &str = "Git address of an extension";
const BTN_ADD_GIT: &str = "extension-add-git";
const BTN_ADD_FOLDER: &str = "extension-add-folder";
const ADD_LABEL: &str = "Add";
const ADD_FOLDER_LABEL: &str = "Add folder…";
const CHOOSE_LABEL: &str = "Choose folder…";
const INSTALL_LABEL: &str = "Install";
const DETECTED_LABEL: &str = "Use detected";
const ENABLE_LABEL: &str = "Turn on";
const DISABLE_LABEL: &str = "Turn off";
const UPDATE_LABEL: &str = "Update";
const REMOVE_LABEL: &str = "Remove";
const FOLDER_PROMPT: &str = "Choose";
const NOT_A_SERVER_RUNTIME_TEXT: &str = "That folder has no usable";
const EXTENSION_FAILED_TEXT: &str = "The extension could not be added";
const CHOOSE_PREFIX: &str = "server_runtime-choose";
const INSTALL_PREFIX: &str = "server_runtime-install";
const DETECTED_PREFIX: &str = "server_runtime-detected";
const TOGGLE_PREFIX: &str = "extension-toggle";
const UPDATE_PREFIX: &str = "extension-update";
const REMOVE_PREFIX: &str = "extension-remove";

pub(in crate::environment_shell) struct LanguagesUi {
    url: Entity<InputState>,
    statuses: Vec<ServerRuntimeStatus>,
}

impl EnvironmentShell {
    pub(super) fn languages_ui(window: &mut Window, cx: &mut Context<Self>) -> LanguagesUi {
        LanguagesUi {
            url: cx.new(|cx| InputState::new(window, cx).placeholder(URL_PLACEHOLDER)),
            statuses: Vec::new(),
        }
    }

    pub(super) fn refresh_server_runtime_statuses(&mut self, cx: &mut Context<Self>) {
        let Some(tools) = self.language_tools() else {
            return;
        };
        let chosen = self.settings.server_runtimes.clone();
        let checking = cx.background_spawn(async move { tools.server_runtime_statuses(&chosen) });
        cx.spawn(async move |this, cx| {
            let statuses = checking.await;
            this.update(cx, |shell, cx| {
                if let Some(ui) = shell.settings_ui.as_mut() {
                    ui.languages.statuses = statuses;
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn server_runtimes_changed(&mut self, cx: &mut Context<Self>) {
        self.save_settings(cx);
        self.refresh_server_runtime_statuses(cx);
        self.load_language_tools(cx);
    }

    fn choose_server_runtime(&mut self, id: ServerRuntimeId, name: String, cx: &mut Context<Self>) {
        let Some(tools) = self.language_tools() else {
            return;
        };
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(FOLDER_PROMPT)),
        });
        let chosen = self.settings.server_runtimes.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = answer.await else {
                return;
            };
            let Some(folder) = paths.into_iter().next() else {
                return;
            };
            let (checked, checking) = (folder.clone(), id.clone());
            let accepted = cx
                .background_spawn(async move { tools.accepts_folder(&checking, &checked, &chosen) })
                .await;
            this.update(cx, |shell, cx| {
                if accepted {
                    shell.settings.server_runtimes.choose(id, folder);
                    shell.server_runtimes_changed(cx);
                } else {
                    shell.toast(&format!("{NOT_A_SERVER_RUNTIME_TEXT} {name}"), cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn install_server_runtime_from_settings(
        &mut self,
        id: ServerRuntimeId,
        cx: &mut Context<Self>,
    ) {
        let Some(tools) = self.language_tools() else {
            return;
        };
        let chosen = self.settings.server_runtimes.clone();
        let installing =
            cx.background_spawn(async move { tools.install_server_runtime(&id, &chosen) });
        cx.spawn(async move |this, cx| {
            let installed = installing.await;
            this.update(cx, |shell, cx| {
                if let Err(error) = installed {
                    shell.toast(&error.to_string(), cx);
                }
                shell.server_runtimes_changed(cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn extensions_changed(&mut self, cx: &mut Context<Self>) {
        self.save_settings(cx);
        self.load_language_tools(cx);
        cx.notify();
    }

    fn add_extension(&mut self, entry: Result<ExtensionEntry, String>, cx: &mut Context<Self>) {
        match entry {
            Ok(entry) => {
                self.settings
                    .extensions
                    .retain(|known| known.id != entry.id);
                self.settings.extensions.push(entry);
                self.extensions_changed(cx);
            }
            Err(error) => self.toast(&format!("{EXTENSION_FAILED_TEXT}: {error}"), cx),
        }
    }

    fn add_git_extension(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(ui), Some(config)) = (self.settings_ui.as_ref(), krad_store::config_dir()) else {
            return;
        };
        let field = ui.languages.url.clone();
        let url = field.read(cx).value().trim().to_owned();
        if url.is_empty() {
            return;
        }
        field.update(cx, |field, cx| field.set_value("", window, cx));
        let cloning = cx.background_spawn(async move {
            add_git(&config, &url).map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let entry = cloning.await;
            this.update(cx, |shell, cx| shell.add_extension(entry, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn add_folder_extension(cx: &mut Context<Self>) {
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(FOLDER_PROMPT)),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = answer.await else {
                return;
            };
            let Some(folder) = paths.into_iter().next() else {
                return;
            };
            let entry = add_folder(&folder).map_err(|error| error.to_string());
            this.update(cx, |shell, cx| shell.add_extension(entry, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn change_extension(&mut self, id: &str, change: ExtensionChange, cx: &mut Context<Self>) {
        let Some(config) = krad_store::config_dir() else {
            return;
        };
        let Some(entry) = self
            .settings
            .extensions
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
        else {
            return;
        };
        match change {
            ExtensionChange::Toggle => {
                if let Some(known) = self
                    .settings
                    .extensions
                    .iter_mut()
                    .find(|known| known.id == id)
                {
                    known.enabled = !known.enabled;
                }
                self.extensions_changed(cx);
            }
            ExtensionChange::Update | ExtensionChange::Remove => {
                let removing = change == ExtensionChange::Remove;
                let working = cx.background_spawn(async move {
                    if removing {
                        remove(&config, &entry)
                    } else {
                        update(&config, &entry)
                    }
                });
                let id = id.to_owned();
                cx.spawn(async move |this, cx| {
                    let done = working.await;
                    this.update(cx, |shell, cx| {
                        if let Err(error) = done {
                            shell.toast(&error.to_string(), cx);
                        }
                        if removing {
                            shell.settings.extensions.retain(|known| known.id != id);
                        }
                        shell.extensions_changed(cx);
                    })
                    .unwrap_or_default();
                })
                .detach();
            }
        }
    }

    fn server_runtime_row(
        index: usize,
        status: &ServerRuntimeStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let detail = status.found.as_ref().map_or_else(
            || NOT_FOUND_TEXT.to_owned(),
            |found| {
                if status.chosen {
                    format!("{} ({CHOSEN_TEXT})", found.display())
                } else {
                    found.display().to_string()
                }
            },
        );
        let (id, name) = (status.id.clone(), status.name.clone());
        let (install_id, forget_id) = (status.id.clone(), status.id.clone());
        let controls = div()
            .flex()
            .flex_row()
            .gap(px(SPACE_2))
            .child(
                Button::new((CHOOSE_PREFIX, index))
                    .label(CHOOSE_LABEL)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.choose_server_runtime(id.clone(), name.clone(), cx);
                    })),
            )
            .children(status.chosen.then(|| {
                Button::new((DETECTED_PREFIX, index))
                    .label(DETECTED_LABEL)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.settings.server_runtimes.forget(&forget_id);
                        this.server_runtimes_changed(cx);
                    }))
            }))
            .children((status.found.is_none() && status.installable).then(|| {
                Button::new((INSTALL_PREFIX, index))
                    .label(INSTALL_LABEL)
                    .small()
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.install_server_runtime_from_settings(install_id.clone(), cx);
                    }))
            }))
            .into_any_element();
        form_row(&status.name, Some(&detail), controls, pal)
    }

    fn extension_row(index: usize, entry: &ExtensionEntry, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let source = match &entry.source {
            ExtensionSource::Git(url) => url.clone(),
            ExtensionSource::Folder(folder) => folder.display().to_string(),
        };
        let (toggle, refresh, drop) = (entry.id.clone(), entry.id.clone(), entry.id.clone());
        let controls = div()
            .flex()
            .flex_row()
            .gap(px(SPACE_2))
            .child(
                Button::new((TOGGLE_PREFIX, index))
                    .label(if entry.enabled {
                        DISABLE_LABEL
                    } else {
                        ENABLE_LABEL
                    })
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.change_extension(&toggle, ExtensionChange::Toggle, cx);
                    })),
            )
            .child(
                Button::new((UPDATE_PREFIX, index))
                    .label(UPDATE_LABEL)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.change_extension(&refresh, ExtensionChange::Update, cx);
                    })),
            )
            .child(
                Button::new((REMOVE_PREFIX, index))
                    .label(REMOVE_LABEL)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.change_extension(&drop, ExtensionChange::Remove, cx);
                    })),
            )
            .into_any_element();
        form_row(&entry.id, Some(&source), controls, pal)
    }

    pub(super) fn languages_pane(&self, ui: &LanguagesUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let server_runtimes: Vec<AnyElement> = ui
            .statuses
            .iter()
            .enumerate()
            .map(|(index, status)| Self::server_runtime_row(index, status, cx))
            .collect();
        let extensions: Vec<AnyElement> = self
            .settings
            .extensions
            .iter()
            .enumerate()
            .map(|(index, entry)| Self::extension_row(index, entry, cx))
            .collect();
        let manifests = krad_store::config_dir().map_or_else(String::new, |config| {
            user_manifests_dir(&config).display().to_string()
        });
        let add = div()
            .flex()
            .flex_row()
            .gap(px(SPACE_2))
            .child(div().flex_1().child(Input::new(&ui.url)))
            .child(
                Button::new(BTN_ADD_GIT)
                    .label(ADD_LABEL)
                    .small()
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.add_git_extension(window, cx);
                    })),
            )
            .child(
                Button::new(BTN_ADD_FOLDER)
                    .label(ADD_FOLDER_LABEL)
                    .small()
                    .on_click(cx.listener(|_: &mut Self, _, _, cx| Self::add_folder_extension(cx))),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .child(self.toolchain_activation_card(cx))
            .child(hint(SERVER_RUNTIMES_HINT, pal))
            .child(panel_card(server_runtimes, pal))
            .child(hint(&format!("{EXTENSIONS_HINT} {manifests}."), pal))
            .child(add)
            .children((!extensions.is_empty()).then(|| panel_card(extensions, pal)))
            .into_any_element()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExtensionChange {
    Toggle,
    Update,
    Remove,
}
