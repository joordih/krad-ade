use gpui_kit::component::input::EditorState;
use gpui_kit::{AppContext as _, Context, Entity, PathPromptOptions, SharedString};
use krad_domain::{TerminalId, ToolchainId};
use krad_extensions::{manifest_folders, LanguageTools, Registry};
use std::path::PathBuf;
use std::sync::Arc;

use super::super::EnvironmentShell;
use super::{ServerKey, ServerSlot};

const FOLDER_PROMPT: &str = "Choose";
const NOT_IN_FOLDER_TEXT: &str = "has no usable";
const MANIFEST_PROBLEM_TEXT: &str = "A language manifest could not be read";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn load_language_tools(&mut self, cx: &mut Context<Self>) {
        let Some(config) = krad_store::config_dir() else {
            return;
        };
        let folders = manifest_folders(&config, &self.settings.extensions);
        let loading = cx.background_spawn(async move { LanguageTools::load(&config, &folders) });
        cx.spawn(async move |this, cx| {
            let Ok(tools) = loading.await else {
                return;
            };
            this.update(cx, |shell, cx| shell.use_language_tools(tools, true, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn use_language_tools(
        &mut self,
        tools: LanguageTools,
        may_refresh: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(problem) = tools.problems().first() {
            self.toast(&format!("{MANIFEST_PROBLEM_TEXT}: {problem}"), cx);
        }
        let stale = tools.registry_is_stale();
        self.language_servers.catalog = tools.catalog().clone();
        self.language_servers.tools = Some(Arc::new(tools));
        self.language_servers
            .slots
            .retain(|_, slot| matches!(slot, ServerSlot::Running(_)));
        self.attach_open_editors(cx);
        if stale && may_refresh {
            self.refresh_registry(cx);
        }
        cx.notify();
    }

    fn refresh_registry(&mut self, cx: &mut Context<Self>) {
        let (Some(tools), Some(config)) = (
            self.language_servers.tools.clone(),
            krad_store::config_dir(),
        ) else {
            return;
        };
        let folders = manifest_folders(&config, &self.settings.extensions);
        let fetching = cx.background_spawn(async move {
            let registry: Registry = tools.refreshed_registry()?;
            Ok::<_, krad_extensions::ExtensionError>(
                LanguageTools::load(&config, &folders)?.with_registry(registry),
            )
        });
        cx.spawn(async move |this, cx| {
            if let Ok(tools) = fetching.await {
                this.update(cx, |shell, cx| shell.use_language_tools(tools, false, cx))
                    .unwrap_or_default();
            }
        })
        .detach();
    }

    fn attach_open_editors(&mut self, cx: &mut Context<Self>) {
        let open: Vec<(TerminalId, PathBuf, Entity<EditorState>)> =
            self.envs.get(&self.active).map_or_else(Vec::new, |live| {
                live.editors
                    .iter()
                    .filter_map(|(id, editor)| {
                        Some((*id, editor.path.clone(), editor.state.clone()?))
                    })
                    .collect()
            });
        for (id, path, state) in open {
            self.attach_language_server(id, &path, &state, cx);
        }
    }

    pub(super) fn install_toolchain(
        &mut self,
        key: ServerKey,
        id: ToolchainId,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(tools) = self.language_servers.tools.clone() else {
            return;
        };
        self.set_slot(key.clone(), ServerSlot::InstallingToolchain(name), cx);
        let chosen = self.settings.toolchains.clone();
        let installing = cx.background_spawn(async move { tools.install_toolchain(&id, &chosen) });
        cx.spawn(async move |this, cx| {
            let installed = installing.await;
            this.update(cx, |shell, cx| match installed {
                Ok(()) => shell.retry_server(key, cx),
                Err(error) => shell.set_slot(key, ServerSlot::Failed(error.to_string()), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn choose_toolchain_folder(
        &mut self,
        key: ServerKey,
        id: ToolchainId,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(tools) = self.language_servers.tools.clone() else {
            return;
        };
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(FOLDER_PROMPT)),
        });
        let chosen = self.settings.toolchains.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = answer.await else {
                return;
            };
            let Some(folder) = paths.into_iter().next() else {
                return;
            };
            let checked = folder.clone();
            let checking = id.clone();
            let accepted = cx
                .background_spawn(async move { tools.accepts_folder(&checking, &checked, &chosen) })
                .await;
            this.update(cx, |shell, cx| {
                if accepted {
                    shell.settings.toolchains.choose(id, folder);
                    shell.save_settings(cx);
                    shell.retry_server(key, cx);
                } else {
                    let reason = format!("{} {NOT_IN_FOLDER_TEXT} {name}", folder.display());
                    shell.set_slot(key, ServerSlot::Failed(reason), cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn decline_server(&mut self, key: &ServerKey, cx: &mut Context<Self>) {
        self.language_servers
            .declined
            .insert(key.plan.package.clone());
        self.language_servers.slots.remove(key);
        cx.notify();
    }
}
