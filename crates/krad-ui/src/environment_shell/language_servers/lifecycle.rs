use futures::StreamExt as _;
use gpui_kit::component::input::EditorState;
use gpui_kit::{AppContext as _, Context, Entity};
use krad_domain::ServerLaunch;
use krad_extensions::ServerConfig;
use krad_lsp::lsp_types::ServerCapabilities;
use krad_lsp::{uri_path, LanguageServer, LspError, ServerEvent, ServerSettings};
use std::path::PathBuf;

use super::super::EnvironmentShell;
use super::{RunningServer, ServerKey, ServerSlot};

impl EnvironmentShell {
    pub(super) fn ensure_server(&mut self, key: ServerKey, cx: &mut Context<Self>) {
        let servers = &mut self.language_servers;
        if servers.slots.contains_key(&key) || servers.declined.contains(&key.plan.package) {
            return;
        }
        let Some(tools) = servers.tools.clone() else {
            return;
        };
        servers.slots.insert(key.clone(), ServerSlot::Locating);
        let chosen = self.settings.toolchains.clone();
        let (plan, root) = (key.plan.clone(), key.root.clone());
        let located = cx.background_spawn(async move {
            match tools.missing_toolchain(&plan, &chosen) {
                Some(missing) => Err(missing),
                None => Ok(tools.locate(&plan, &root, &chosen)),
            }
        });
        cx.spawn(async move |this, cx| {
            let located = located.await;
            this.update(cx, |shell, cx| match located {
                Ok(Some(launch)) => shell.start_server(key, launch, cx),
                Ok(None) => shell.set_slot(key, ServerSlot::AwaitingConsent, cx),
                Err(missing) => shell.set_slot(key, ServerSlot::NeedsToolchain(missing), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn set_slot(&mut self, key: ServerKey, slot: ServerSlot, cx: &mut Context<Self>) {
        self.language_servers.slots.insert(key, slot);
        cx.notify();
    }

    pub(super) fn retry_server(&mut self, key: ServerKey, cx: &mut Context<Self>) {
        self.language_servers.slots.remove(&key);
        self.ensure_server(key, cx);
    }

    pub(super) fn install_server(&mut self, key: ServerKey, cx: &mut Context<Self>) {
        let Some(tools) = self.language_servers.tools.clone() else {
            return;
        };
        self.set_slot(key.clone(), ServerSlot::Installing, cx);
        let chosen = self.settings.toolchains.clone();
        let (plan, root) = (key.plan.clone(), key.root.clone());
        let fetched = cx.background_spawn(async move { tools.install(&plan, &root, &chosen) });
        cx.spawn(async move |this, cx| {
            let fetched = fetched.await;
            this.update(cx, |shell, cx| match fetched {
                Ok(launch) => shell.start_server(key, launch, cx),
                Err(error) => shell.set_slot(key, ServerSlot::Failed(error.to_string()), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn server_config(&self, key: &ServerKey) -> ServerConfig {
        self.language_servers
            .tools
            .as_ref()
            .map_or_else(ServerConfig::default, |tools| tools.config(&key.plan))
    }

    fn start_server(&mut self, key: ServerKey, launch: ServerLaunch, cx: &mut Context<Self>) {
        self.set_slot(key.clone(), ServerSlot::Starting, cx);
        let config = self.server_config(&key);
        let settings = ServerSettings::new(config.sections());
        let options = config.options;
        let root = key.root.clone();
        let started =
            cx.background_spawn(async move { LanguageServer::start(&launch, &root, settings) });
        cx.spawn(async move |this, cx| {
            let (server, events) = match started.await {
                Ok(started) => started,
                Err(error) => {
                    this.update(cx, |shell, cx| {
                        shell.set_slot(key, ServerSlot::Failed(error.to_string()), cx);
                    })
                    .unwrap_or_default();
                    return;
                }
            };
            let capabilities =
                LanguageServer::initialize(server.client.clone(), &key.root, options).await;
            this.update(cx, |shell, cx| match capabilities {
                Ok(capabilities) => shell.server_ready(&key, server, capabilities, events, cx),
                Err(error) => shell.set_slot(key, ServerSlot::Failed(error.to_string()), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn server_ready(
        &mut self,
        key: &ServerKey,
        server: LanguageServer,
        capabilities: ServerCapabilities,
        mut events: futures::channel::mpsc::Receiver<ServerEvent>,
        cx: &mut Context<Self>,
    ) {
        let watched = key.clone();
        let listening = cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                let handled = this.update(cx, |shell, cx| shell.server_event(&watched, event, cx));
                if handled.is_err() {
                    return;
                }
            }
        });
        self.language_servers.slots.insert(
            key.clone(),
            ServerSlot::Running(Box::new(RunningServer {
                server,
                capabilities,
                _events: listening,
            })),
        );
        let shell = &*self;
        let open: Vec<(PathBuf, Entity<EditorState>)> = shell
            .envs
            .values()
            .flat_map(|live| {
                live.editors.iter().filter_map(move |(id, editor)| {
                    let state = editor.state.clone()?;
                    (shell.server_key(live, *id, &editor.path).as_ref() == Some(key))
                        .then(|| (editor.path.clone(), state))
                })
            })
            .collect();
        for (path, state) in open {
            self.open_document(key, &path, &state, cx);
        }
        cx.notify();
    }

    fn server_event(&mut self, key: &ServerKey, event: ServerEvent, cx: &mut Context<Self>) {
        match event {
            ServerEvent::Diagnostics(published) => {
                let Some(path) = uri_path(&published.uri) else {
                    return;
                };
                self.language_servers
                    .diagnostics
                    .pushed(path.clone(), published.diagnostics);
                self.show_diagnostics(&path, cx);
            }
            ServerEvent::ApplyEdit(edit) => self.queue_workspace_edit(edit, cx),
            ServerEvent::DiagnosticsRefresh => {
                let served: Vec<PathBuf> = self
                    .language_servers
                    .documents
                    .iter()
                    .filter(|(_, served)| *served == key)
                    .map(|(path, _)| path.clone())
                    .collect();
                for path in served {
                    self.pull_diagnostics(&path, cx);
                }
            }
            ServerEvent::Stopped => {
                self.language_servers
                    .documents
                    .retain(|_, served| served != key);
                self.set_slot(
                    key.clone(),
                    ServerSlot::Failed(LspError::Stopped.to_string()),
                    cx,
                );
            }
        }
    }
}
