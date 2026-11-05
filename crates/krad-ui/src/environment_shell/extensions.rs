mod activation;
mod calls;
mod commands;
mod events;
mod keys;
mod lifecycle;
mod machines;
mod matching;
mod progress;
mod prompts;
mod runtime_install;
mod terminals;
mod tools;
mod trust;
mod views;

pub(in crate::environment_shell) use calls::default_value;
pub(in crate::environment_shell) use commands::{ExtensionMenu, ExtensionPick, PaletteEntry};
pub(in crate::environment_shell) use keys::{ClashWith, KeyConflict};
pub(in crate::environment_shell) use machines::{HostKindRef, MachineDialog};
pub(in crate::environment_shell) use views::OpenViewId;

use gpui_kit::{AppContext as _, Context, Task};
use krad_domain::{EnvironmentName, ExtensionEntry, ExtensionSettings, TerminalId};
use krad_extensions::api::manifest::{ExtensionManifest, Placement};
use krad_extensions::api::{ExtensionId, TerminalHandle};
use krad_extensions::folder_of;
use krad_extensions::program::{
    read_installed, ExtensionProcess, InstalledExtension, ProgramConnection, ProgramLog,
    RestartPolicy, Trust,
};
use std::collections::{HashMap, HashSet, VecDeque};

use super::EnvironmentShell;

const EXTENSIONS_NOT_READ: &str = "An extension could not be read";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::environment_shell) struct SlotKey {
    pub(in crate::environment_shell) extension: ExtensionId,
    pub(in crate::environment_shell) environment: Option<EnvironmentName>,
}

#[derive(Clone, Debug)]
pub(in crate::environment_shell) enum Loaded {
    ManifestsOnly,
    Program(Box<InstalledExtension>),
    Problem(String),
}

#[derive(Clone, Debug)]
pub(in crate::environment_shell) struct ExtensionRecord {
    pub(in crate::environment_shell) entry: ExtensionEntry,
    pub(in crate::environment_shell) loaded: Loaded,
    pub(in crate::environment_shell) log: ProgramLog,
}

impl ExtensionRecord {
    pub(in crate::environment_shell) fn installed(&self) -> Option<&InstalledExtension> {
        match &self.loaded {
            Loaded::Program(installed) => Some(installed),
            Loaded::ManifestsOnly | Loaded::Problem(_) => None,
        }
    }

    pub(in crate::environment_shell) fn manifest(&self) -> Option<&ExtensionManifest> {
        self.installed().map(|installed| &installed.manifest)
    }

    pub(in crate::environment_shell) fn trust(&self) -> Trust {
        self.installed()
            .map_or(Trust::NotNeeded, |installed| installed.trust(&self.entry))
    }

    pub(in crate::environment_shell) fn usable(&self) -> bool {
        self.entry.enabled && self.trust() == Trust::Trusted
    }

    pub(in crate::environment_shell) fn title(&self) -> String {
        self.manifest()
            .map_or_else(|| self.entry.id.clone(), |manifest| manifest.name.clone())
    }
}

pub(in crate::environment_shell) enum Deferred {
    Command(
        krad_extensions::api::CommandId,
        Option<krad_extensions::api::protocol::CommandTarget>,
    ),
    Render(OpenViewId),
    Tool(tools::ToolCall),
    Machines(machines::MachineRequest),
}

pub(in crate::environment_shell) struct Running {
    pub(in crate::environment_shell) process: ExtensionProcess,
    _events: Task<()>,
}

pub(in crate::environment_shell) enum Slot {
    WaitingForTools(Vec<Deferred>),
    Starting(Vec<Deferred>),
    Initializing(Running, Vec<Deferred>),
    Running(Running),
    Failed(String),
    Unsupported,
}

#[derive(Default)]
pub(in crate::environment_shell) struct Extensions {
    pub(in crate::environment_shell) records: Vec<ExtensionRecord>,
    pub(in crate::environment_shell) slots: HashMap<SlotKey, Slot>,
    restarts: HashMap<SlotKey, RestartPolicy>,
    pub(in crate::environment_shell) values: ExtensionSettings,
    views: views::OpenViews,
    prompts: prompts::Prompts,
    progress: progress::ProgressBoard,
    terminals: HashMap<TerminalHandle, (EnvironmentName, TerminalId)>,
    view_terminals: Vec<(SlotKey, TerminalHandle, gpui_kit::AnyView)>,
    next_terminal: u64,
    pub(in crate::environment_shell) conflicts: Vec<KeyConflict>,
    bound: HashSet<(String, String, String)>,
    declined: HashSet<String>,
    trust_queue: VecDeque<String>,
    restored: HashSet<EnvironmentName>,
    watch: activation::Watch,
    loaded: bool,
    machines: machines::Machines,
}

impl Extensions {
    pub(in crate::environment_shell) fn record(
        &self,
        id: &ExtensionId,
    ) -> Option<&ExtensionRecord> {
        self.records
            .iter()
            .find(|record| record.manifest().is_some_and(|manifest| manifest.id == *id))
    }

    pub(in crate::environment_shell) fn title_of(&self, id: &ExtensionId) -> String {
        self.record(id)
            .map_or_else(String::new, ExtensionRecord::title)
    }

    fn connection(&self, key: &SlotKey) -> Option<ProgramConnection> {
        match self.slots.get(key) {
            Some(Slot::Running(running) | Slot::Initializing(running, _)) => {
                Some(running.process.connection.clone())
            }
            _ => None,
        }
    }

    pub(in crate::environment_shell) fn slot_state(&self, id: &ExtensionId) -> Option<&Slot> {
        self.slots
            .iter()
            .find(|(key, _)| key.extension == *id)
            .map(|(_, slot)| slot)
    }
}

fn read_records(config: &std::path::Path, entries: Vec<ExtensionEntry>) -> Vec<ExtensionRecord> {
    entries
        .into_iter()
        .map(|entry| {
            let folder = folder_of(config, &entry);
            let loaded = match read_installed(&folder) {
                Ok(Some(installed)) => Loaded::Program(Box::new(installed)),
                Ok(None) => Loaded::ManifestsOnly,
                Err(error) => Loaded::Problem(error.to_string()),
            };
            ExtensionRecord {
                entry,
                loaded,
                log: ProgramLog::default(),
            }
        })
        .collect()
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn load_extensions(&mut self, cx: &mut Context<Self>) {
        let Some(config) = krad_store::config_dir() else {
            return;
        };
        let entries = self.settings.extensions.clone();
        let reading = cx.background_spawn(async move {
            let records = read_records(&config, entries);
            let values = krad_store::read_extension_settings();
            (records, values)
        });
        cx.spawn(async move |this, cx| {
            let (records, values) = reading.await;
            this.update(cx, |shell, cx| shell.use_extensions(records, values, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn use_extensions(
        &mut self,
        records: Vec<ExtensionRecord>,
        values: ExtensionSettings,
        cx: &mut Context<Self>,
    ) {
        let kept: Vec<ExtensionRecord> = records
            .into_iter()
            .map(|mut record| {
                if let Some(previous) = self
                    .extensions
                    .records
                    .iter()
                    .find(|previous| previous.entry.id == record.entry.id)
                {
                    record.log = previous.log.clone();
                }
                record
            })
            .collect();
        if let Some(problem) = kept.iter().find_map(|record| match &record.loaded {
            Loaded::Problem(problem) if record.entry.enabled => Some(problem.clone()),
            _ => None,
        }) {
            self.toast(&format!("{EXTENSIONS_NOT_READ}: {problem}"), cx);
        }
        self.stop_changed_programs(&kept, cx);
        self.extensions.records = kept;
        self.extensions.values = values;
        self.extensions.loaded = true;
        self.bind_extension_keys(cx);
        self.queue_trust_prompts(cx);
        self.activate_on_startup(cx);
        cx.notify();
    }

    fn stop_changed_programs(&mut self, next: &[ExtensionRecord], cx: &mut Context<Self>) {
        let still = |key: &SlotKey, previous: Option<&ExtensionRecord>| {
            let Some(previous) = previous.and_then(ExtensionRecord::installed) else {
                return false;
            };
            next.iter().any(|record| {
                record.usable()
                    && record.installed().is_some_and(|installed| {
                        installed.manifest.id == key.extension
                            && installed.fingerprint() == previous.fingerprint()
                    })
            })
        };
        let stopped: Vec<SlotKey> = self
            .extensions
            .slots
            .keys()
            .filter(|key| !still(key, self.extensions.record(&key.extension)))
            .cloned()
            .collect();
        for key in stopped {
            self.stop_program(&key, cx);
        }
    }

    pub(in crate::environment_shell) fn stop_program(
        &mut self,
        key: &SlotKey,
        cx: &mut Context<Self>,
    ) {
        self.extensions.slots.remove(key);
        self.extensions.restarts.remove(key);
        self.extensions
            .view_terminals
            .retain(|(owner, _, _)| owner != key);
        self.drop_views_of(key, cx);
    }

    fn slot_key(&self, manifest: &ExtensionManifest) -> Option<SlotKey> {
        let placement = manifest.program.as_ref()?.placement;
        let environment = match placement {
            Placement::App => None,
            Placement::Environment => Some(self.active.clone()),
        };
        Some(SlotKey {
            extension: manifest.id.clone(),
            environment,
        })
    }

    pub(in crate::environment_shell) fn usable_manifests(&self) -> Vec<ExtensionManifest> {
        self.extensions
            .records
            .iter()
            .filter(|record| record.usable())
            .filter_map(|record| record.manifest().cloned())
            .collect()
    }
}
