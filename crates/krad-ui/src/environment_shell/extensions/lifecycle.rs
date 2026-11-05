use futures::channel::mpsc;
use futures::StreamExt as _;
use gpui_kit::{AppContext as _, Context};
use krad_domain::{Host, ServerRuntimeId};
use krad_extensions::api::manifest::Placement;
use krad_extensions::api::protocol::{
    AppInfo, EnvironmentInfo, ExecuteCommand, ExecuteCommandParams, Feature, HostInfo,
    InitializeParams,
};
use krad_extensions::api::CURRENT_API;
use krad_extensions::program::{initialize, ExtensionProcess, ProgramEvent, RestartPolicy, Trust};
use krad_extensions::ExtensionError;
use krad_host::{HostLink, HostPlatform};
use std::collections::BTreeSet;

use super::super::EnvironmentShell;
use super::{Deferred, Running, Slot, SlotKey};

const APP_NAME: &str = krad_store::APP_NAME;
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const FEATURES: [Feature; 10] = [
    Feature::Views,
    Feature::Commands,
    Feature::Window,
    Feature::Terminals,
    Feature::Files,
    Feature::Workspace,
    Feature::Settings,
    Feature::Secrets,
    Feature::Tools,
    Feature::HostKinds,
];
const NOT_TRUSTED_TEXT: &str = "is not trusted yet";
const COULD_NOT_START_TEXT: &str = "could not start";
const GAVE_UP_TEXT: &str = "stopped too often and was not restarted";
const FAILED_TEXT: &str = "has failed; retry it from Settings";
const COMMAND_FAILED_TEXT: &str = "command failed";
const NEEDS_RUNTIME_TEXT: &str = "needs";

pub(super) enum StartFailure {
    MissingRuntime(ServerRuntimeId),
    NoBuild,
    Other(String),
}

impl From<ExtensionError> for StartFailure {
    fn from(error: ExtensionError) -> Self {
        match error {
            ExtensionError::MissingServerRuntime(runtime) => Self::MissingRuntime(runtime),
            ExtensionError::NoBuild => Self::NoBuild,
            other => Self::Other(other.to_string()),
        }
    }
}
const NO_BUILD_TEXT: &str = "has no build for this environment's host";
const HOST_GONE_TEXT: &str = "cannot reach this environment's host";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn ensure_running(
        &mut self,
        key: SlotKey,
        deferred: Deferred,
        cx: &mut Context<Self>,
    ) {
        let title = self.extensions.title_of(&key.extension);
        match self.extensions.slots.get_mut(&key) {
            Some(Slot::Running(_)) => {
                self.run_deferred(&key, deferred, cx);
                return;
            }
            Some(Slot::Starting(waiting) | Slot::WaitingForTools(waiting)) => {
                waiting.push(deferred);
                return;
            }
            Some(Slot::Failed(_)) => {
                self.toast(&format!("{title} {FAILED_TEXT}"), cx);
                return;
            }
            Some(Slot::Unsupported) => {
                self.toast(&format!("{title} {NO_BUILD_TEXT}"), cx);
                return;
            }
            None => {}
        }
        self.start_program(key, vec![deferred], cx);
    }

    pub(in crate::environment_shell) fn start_program(
        &mut self,
        key: SlotKey,
        waiting: Vec<Deferred>,
        cx: &mut Context<Self>,
    ) {
        let Some(record) = self.extensions.record(&key.extension).cloned() else {
            return;
        };
        let Some(installed) = record.installed().cloned() else {
            return;
        };
        if record.trust() != Trust::Trusted {
            self.toast(&format!("{} {NOT_TRUSTED_TEXT}", record.title()), cx);
            self.ask_trust(&record.entry.id, cx);
            return;
        }
        let Some((link, platform)) = self.program_link(&key) else {
            self.toast(&format!("{} {HOST_GONE_TEXT}", record.title()), cx);
            return;
        };
        let Some(tools) = self.language_tools() else {
            self.extensions
                .slots
                .insert(key, Slot::WaitingForTools(waiting));
            return;
        };
        let placement = installed
            .manifest
            .program
            .as_ref()
            .map_or(Placement::App, |program| program.placement);
        let params = InitializeParams {
            app: AppInfo {
                name: APP_NAME.to_owned(),
                version: APP_VERSION.to_owned(),
            },
            api: CURRENT_API,
            placement,
            extension_folder: installed.folder.clone(),
            environment: key
                .environment
                .as_ref()
                .and_then(|name| self.environment_info(name)),
            features: FEATURES.into_iter().collect::<BTreeSet<_>>(),
        };
        self.extensions
            .slots
            .insert(key.clone(), Slot::Starting(waiting));
        let chosen = self.settings.server_runtimes.clone();
        let log = record.log.clone();
        let starting = cx.background_spawn(async move {
            let mut params = params;
            let site = tools
                .program_site(&link, platform, &installed, &chosen)
                .map_err(StartFailure::from)?;
            params.extension_folder.clone_from(&site.folder);
            let (process, events) =
                ExtensionProcess::start_on(&link, &site.launch, &site.folder, log)
                    .map_err(|error| StartFailure::Other(error.to_string()))?;
            initialize(&process.connection, &params)
                .await
                .map_err(|error| StartFailure::Other(error.to_string()))?;
            Ok::<_, StartFailure>((process, events))
        });
        cx.spawn(async move |this, cx| {
            let started = starting.await;
            this.update(cx, |shell, cx| shell.program_started(&key, started, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn environment_info(
        &self,
        name: &krad_domain::EnvironmentName,
    ) -> Option<EnvironmentInfo> {
        let live = self.envs.get(name)?;
        let host = match &live.data.host {
            Host::Local => HostInfo::Local,
            Host::Ssh { destination } => HostInfo::Ssh {
                destination: destination.to_string(),
            },
            Host::Extension { machine } => HostInfo::Machine {
                extension: machine.extension.clone(),
                host_kind: machine.kind.clone(),
                machine: machine.id.clone(),
                label: machine.label.clone(),
            },
        };
        Some(EnvironmentInfo {
            id: name.as_str().to_owned(),
            name: self.env_label(name),
            host,
        })
    }

    pub(super) fn program_link(&self, key: &SlotKey) -> Option<(HostLink, Option<HostPlatform>)> {
        let Some(name) = &key.environment else {
            return Some((HostLink::Local, None));
        };
        let live = self.envs.get(name)?;
        if matches!(live.data.host, Host::Local) {
            return Some((HostLink::Local, None));
        }
        let platform = match &live.link {
            HostLink::Local => None,
            HostLink::Remote(remote) => remote.platform(),
        };
        Some((live.link.clone(), platform))
    }

    fn program_started(
        &mut self,
        key: &SlotKey,
        started: Result<(ExtensionProcess, mpsc::Receiver<ProgramEvent>), StartFailure>,
        cx: &mut Context<Self>,
    ) {
        let Some(Slot::Starting(waiting)) = self.extensions.slots.remove(key) else {
            return;
        };
        let title = self.extensions.title_of(&key.extension);
        let (process, mut events) = match started {
            Ok(started) => started,
            Err(StartFailure::NoBuild) => {
                self.toast(&format!("{title} {NO_BUILD_TEXT}"), cx);
                self.extensions.slots.insert(key.clone(), Slot::Unsupported);
                self.fail_views_of(key, cx);
                return;
            }
            Err(StartFailure::MissingRuntime(runtime)) => {
                let error = format!("{title} {NEEDS_RUNTIME_TEXT} {runtime}");
                self.extensions
                    .slots
                    .insert(key.clone(), Slot::Failed(error));
                self.offer_runtime_install(key, runtime, waiting, cx);
                return;
            }
            Err(StartFailure::Other(error)) => {
                if let Some(record) = self.extensions.record(&key.extension) {
                    record.log.push(error.clone());
                }
                self.toast(&format!("{title} {COULD_NOT_START_TEXT}: {error}"), cx);
                self.extensions
                    .slots
                    .insert(key.clone(), Slot::Failed(error));
                self.fail_views_of(key, cx);
                return;
            }
        };
        tracing::info!(extension = %key.extension, "extension program running");
        let listening = key.clone();
        let task = cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                let handled = this.update(cx, |shell, cx| {
                    shell.program_event(&listening, event, cx);
                });
                if handled.is_err() {
                    break;
                }
            }
        });
        self.extensions.slots.insert(
            key.clone(),
            Slot::Running(Running {
                process,
                _events: task,
            }),
        );
        for deferred in waiting {
            self.run_deferred(key, deferred, cx);
        }
        self.render_views_of(key, cx);
        cx.notify();
    }

    fn program_event(&mut self, key: &SlotKey, event: ProgramEvent, cx: &mut Context<Self>) {
        match event {
            ProgramEvent::Call { id, call } => self.handle_call(key, id, call, cx),
            ProgramEvent::Notice(notice) => self.handle_notice(key, notice, cx),
            ProgramEvent::Stopped => self.program_stopped(key, cx),
        }
        cx.notify();
    }

    fn program_stopped(&mut self, key: &SlotKey, cx: &mut Context<Self>) {
        let Some(Slot::Running(running)) = self.extensions.slots.remove(key) else {
            return;
        };
        let ran_for = running.process.ran_for();
        drop(running);
        self.extensions.progress.forget(key);
        tracing::info!(extension = %key.extension, "extension program stopped");
        let delay = self
            .extensions
            .restarts
            .entry(key.clone())
            .or_default()
            .after_crash(ran_for);
        let Some(delay) = delay else {
            let title = self.extensions.title_of(&key.extension);
            self.toast(&format!("{title} {GAVE_UP_TEXT}"), cx);
            self.extensions
                .slots
                .insert(key.clone(), Slot::Failed(GAVE_UP_TEXT.to_owned()));
            self.fail_views_of(key, cx);
            return;
        };
        self.extensions
            .slots
            .insert(key.clone(), Slot::Starting(Vec::new()));
        let restarting = key.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            this.update(cx, |shell, cx| {
                if matches!(shell.extensions.slots.get(&restarting), Some(Slot::Starting(waiting)) if waiting.is_empty())
                {
                    shell.extensions.slots.remove(&restarting);
                    shell.start_program(restarting, Vec::new(), cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn retry_extension(
        &mut self,
        id: &krad_extensions::api::ExtensionId,
        cx: &mut Context<Self>,
    ) {
        let failed: Vec<SlotKey> = self
            .extensions
            .slots
            .iter()
            .filter(|(key, slot)| {
                key.extension == *id && matches!(slot, Slot::Failed(_) | Slot::Unsupported)
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in &failed {
            self.extensions.slots.remove(key);
            self.extensions
                .restarts
                .insert(key.clone(), RestartPolicy::default());
        }
        let key = failed.into_iter().next().or_else(|| {
            self.extensions
                .record(id)
                .and_then(super::ExtensionRecord::manifest)
                .and_then(|manifest| self.slot_key(manifest))
        });
        if let Some(key) = key {
            self.start_program(key, Vec::new(), cx);
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn resume_extension_starts(&mut self, cx: &mut Context<Self>) {
        let waiting: Vec<SlotKey> = self
            .extensions
            .slots
            .iter()
            .filter(|(_, slot)| matches!(slot, Slot::WaitingForTools(_)))
            .map(|(key, _)| key.clone())
            .collect();
        for key in waiting {
            if let Some(Slot::WaitingForTools(deferred)) = self.extensions.slots.remove(&key) {
                self.start_program(key, deferred, cx);
            }
        }
    }

    pub(in crate::environment_shell) fn run_deferred(
        &mut self,
        key: &SlotKey,
        deferred: Deferred,
        cx: &mut Context<Self>,
    ) {
        match deferred {
            Deferred::Command(command, target) => {
                let Some(connection) = self.extensions.connection(key) else {
                    return;
                };
                let title = self.extensions.title_of(&key.extension);
                cx.spawn(async move |this, cx| {
                    let params = ExecuteCommandParams { command, target };
                    if let Err(error) = connection.request::<ExecuteCommand>(&params).await {
                        this.update(cx, |shell, cx| {
                            shell.toast(&format!("{title}: {COMMAND_FAILED_TEXT}: {error}"), cx);
                        })
                        .unwrap_or_default();
                    }
                })
                .detach();
            }
            Deferred::Render(view) => self.render_view(view, cx),
            Deferred::Tool(call) => self.run_tool_call(key, call, cx),
            Deferred::Machines(request) => self.run_machine_request(key, request, cx),
        }
    }
}
