use futures::StreamExt as _;
use gpui_kit::component::input::EditorState;
use gpui_kit::{AppContext as _, Context, Entity};
use krad_domain::{Host, ServerLaunch};
use krad_extensions::{launch_with, merged_settings, ServerConfig};
use krad_host::{HostLink, HostPlatform};
use krad_lsp::lsp_types::ServerCapabilities;
use krad_lsp::{uri_path, LanguageServer, LspError, ServerEvent, ServerSettings};
use std::path::PathBuf;

use super::super::EnvironmentShell;
use super::toolchain_switch::computed_handoff;
use super::{Document, RunningServer, ServerFailure, ServerKey, ServerSlot, StartId};

pub(super) const HOST_GONE: &str = "No open environment uses this host";

impl EnvironmentShell {
    pub(super) fn server_link(&self, host: &Host) -> Option<(HostLink, Option<HostPlatform>)> {
        if host.is_local() {
            return Some((HostLink::Local, None));
        }
        let link = self
            .envs
            .values()
            .find(|live| live.data.host == *host)?
            .link
            .clone();
        let platform = match &link {
            HostLink::Local => None,
            HostLink::Ssh(ssh) => ssh.platform(),
        };
        Some((link, platform))
    }

    pub(super) fn ensure_server(&mut self, key: ServerKey, cx: &mut Context<Self>) {
        let servers = &mut self.language_servers;
        if servers.slots.contains_key(&key) || servers.declined.contains(&key.plan.package) {
            return;
        }
        let Some(tools) = servers.tools.clone() else {
            return;
        };
        let chosen = self.settings.server_runtimes.clone();
        let (plan, root) = (key.plan.clone(), key.root.clone());
        let Some((link, platform)) = self.server_link(&key.host) else {
            return;
        };
        self.language_servers
            .slots
            .insert(key.clone(), ServerSlot::Locating);
        let located = cx.background_spawn(async move {
            match tools.missing_server_runtime_on(&link, platform, &plan, &chosen) {
                Some(missing) => Err(missing),
                None => Ok(tools.locate_on(&link, platform, &plan, &root, &chosen)),
            }
        });
        cx.spawn(async move |this, cx| {
            let located = located.await;
            this.update(cx, |shell, cx| match located {
                Ok(Some(launch)) => shell.start_server(key, launch, cx),
                Ok(None) => shell.set_slot(key, ServerSlot::AwaitingConsent, cx),
                Err(missing) => shell.set_slot(key, ServerSlot::NeedsServerRuntime(missing), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn set_slot(&mut self, key: ServerKey, slot: ServerSlot, cx: &mut Context<Self>) {
        self.language_servers.shown_details.remove(&key);
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
        let chosen = self.settings.server_runtimes.clone();
        let (plan, root) = (key.plan.clone(), key.root.clone());
        let Some((link, platform)) = self.server_link(&key.host) else {
            self.set_slot(
                key,
                ServerSlot::Failed(ServerFailure::reason(HOST_GONE)),
                cx,
            );
            return;
        };
        let fetched =
            cx.background_spawn(
                async move { tools.install_on(&link, platform, &plan, &root, &chosen) },
            );
        cx.spawn(async move |this, cx| {
            let fetched = fetched.await;
            this.update(cx, |shell, cx| match fetched {
                Ok(launch) => shell.start_server(key, launch, cx),
                Err(error) => shell.set_slot(key, ServerSlot::Failed((&error).into()), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn server_config(&self, key: &ServerKey) -> ServerConfig {
        self.language_servers
            .tools
            .as_ref()
            .map_or_else(ServerConfig::default, |tools| tools.config(&key.plan))
    }

    fn start_server(&mut self, key: ServerKey, launch: ServerLaunch, cx: &mut Context<Self>) {
        let start = self.language_servers.last_start.next();
        self.language_servers.last_start = start;
        self.set_slot(key.clone(), ServerSlot::Starting(start), cx);
        let config = self.server_config(&key);
        let sections = config.sections();
        let options = config.options;
        let root = key.root.clone();
        let Some((link, _)) = self.server_link(&key.host) else {
            self.set_slot(
                key,
                ServerSlot::Failed(ServerFailure::reason(HOST_GONE)),
                cx,
            );
            return;
        };
        let handing = self.handoff_inputs(&key);
        let starting = link.clone();
        let started = cx.background_spawn(async move {
            let (launch, sections) = match &handing {
                Some(inputs) => {
                    let handed = computed_handoff(inputs);
                    (
                        launch_with(launch, &handed, &inputs.link),
                        merged_settings(sections, &handed),
                    )
                }
                None => (launch, sections),
            };
            LanguageServer::start(&starting, &launch, &root, ServerSettings::new(sections))
        });
        cx.spawn(async move |this, cx| {
            let (server, events) = match started.await {
                Ok(started) => started,
                Err(error) => {
                    this.update(cx, |shell, cx| {
                        if shell.still_starting(&key, start) {
                            shell.set_slot(
                                key,
                                ServerSlot::Failed(ServerFailure::reason(error.to_string())),
                                cx,
                            );
                        }
                    })
                    .unwrap_or_default();
                    return;
                }
            };
            let capabilities =
                LanguageServer::initialize(server.client.clone(), &link, &key.root, options).await;
            this.update(cx, |shell, cx| {
                if !shell.still_starting(&key, start) {
                    tracing::debug!("dropped a language server whose start was superseded");
                    return;
                }
                match capabilities {
                    Ok(capabilities) => shell.server_ready(&key, server, capabilities, events, cx),
                    Err(error) => shell.set_slot(
                        key,
                        ServerSlot::Failed(ServerFailure::reason(error.to_string())),
                        cx,
                    ),
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn still_starting(&self, key: &ServerKey, start: StartId) -> bool {
        matches!(
            self.language_servers.slots.get(key),
            Some(ServerSlot::Starting(current)) if *current == start
        )
    }

    fn server_ready(
        &mut self,
        key: &ServerKey,
        server: LanguageServer,
        capabilities: ServerCapabilities,
        mut events: futures::channel::mpsc::Receiver<ServerEvent>,
        cx: &mut Context<Self>,
    ) {
        let open = self.documents_for(key);
        if open.is_empty() {
            self.language_servers.slots.remove(key);
            tracing::debug!("dropped a language server no open file needs any more");
            cx.notify();
            return;
        }
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
                progress: super::progress::ServerProgress::default(),
                _events: listening,
            })),
        );
        for (path, state) in open {
            self.open_document(key, &path, &state, cx);
        }
        cx.notify();
    }

    fn documents_for(&self, key: &ServerKey) -> Vec<(PathBuf, Entity<EditorState>)> {
        self.envs
            .values()
            .flat_map(|live| {
                live.editors.iter().filter_map(move |(id, editor)| {
                    let state = editor.state.clone()?;
                    (self.server_key(live, *id, &editor.path).as_ref() == Some(key))
                        .then(|| (editor.path.clone(), state))
                })
            })
            .collect()
    }

    fn server_event(&mut self, key: &ServerKey, event: ServerEvent, cx: &mut Context<Self>) {
        match event {
            ServerEvent::Diagnostics(published) => {
                let Some(path) = uri_path(&published.uri) else {
                    return;
                };
                let document = Document::new(&key.host, &path);
                self.language_servers
                    .diagnostics
                    .pushed(document.clone(), published.diagnostics);
                self.show_diagnostics(&document, cx);
            }
            ServerEvent::ApplyEdit(edit) => self.queue_workspace_edit(key.host.clone(), edit, cx),
            ServerEvent::DiagnosticsRefresh => {
                let served: Vec<Document> = self
                    .language_servers
                    .documents
                    .iter()
                    .filter(|(_, served)| *served == key)
                    .map(|(document, _)| document.clone())
                    .collect();
                for document in served {
                    self.pull_diagnostics(&document, cx);
                }
            }
            ServerEvent::Progress(progress) => {
                if let Some(ServerSlot::Running(running)) = self.language_servers.slots.get_mut(key)
                {
                    running.progress.apply(progress);
                    cx.notify();
                }
            }
            ServerEvent::Stopped => {
                self.language_servers
                    .documents
                    .retain(|_, served| served != key);
                self.set_slot(
                    key.clone(),
                    ServerSlot::Failed(ServerFailure::reason(LspError::Stopped.to_string())),
                    cx,
                );
            }
        }
    }
}
