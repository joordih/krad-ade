mod dialog;
mod reach;

pub(in crate::environment_shell) use dialog::MachineDialog;
use reach::{host_kind_offers, machine_channel, machine_choices, reach_of, Reach};

use gpui_kit::Context;
use krad_domain::{EnvironmentData, EnvironmentName, ExtensionMachine, Host};
use krad_extensions::api::protocol::{
    ConnectMachine, ConnectMachineParams, ListMachines, ListMachinesParams, Machine,
};
use krad_extensions::api::{ExtensionId, HostKindId, MachineId};
use krad_host::HostLink;
use std::collections::HashMap;

use super::super::host::{Attempt, HostDialog, HostState};
use super::super::{EnvironmentShell, LiveEnv};
use super::{Deferred, Slot, SlotKey};

const CONNECT_FAILED_TEXT: &str = "could not open a channel to";
const LIST_FAILED_TEXT: &str = "could not list its machines";
const STOPPED_TEXT: &str = "stopped before it answered";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct HostKindRef {
    pub(in crate::environment_shell) extension: ExtensionId,
    pub(in crate::environment_shell) kind: HostKindId,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::environment_shell) enum MachineRequest {
    List(HostKindRef),
    Connect(EnvironmentName),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    WaitingForExtensions(Attempt),
    Asking(Attempt),
}

#[derive(Default)]
pub(in crate::environment_shell) struct Machines {
    pending: HashMap<EnvironmentName, (Pending, Option<SlotKey>)>,
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn host_kind_entries(&self) -> Vec<(String, HostKindRef)> {
        host_kind_offers(&self.extensions.records)
    }

    pub(in crate::environment_shell) fn open_machine_dialog(
        &mut self,
        kind: HostKindRef,
        cx: &mut Context<Self>,
    ) {
        let title = self.extensions.title_of(&kind.extension);
        let record = self.extensions.record(&kind.extension);
        let reach = reach_of(record, &kind.kind);
        let mut dialog = MachineDialog::loading(kind.clone(), title.clone());
        if reach != Reach::Reachable {
            dialog.fail(reach.explain(&title));
            self.hosts.dialog = HostDialog::Machines(dialog);
            cx.notify();
            return;
        }
        self.hosts.dialog = HostDialog::Machines(dialog);
        cx.notify();
        let Some(key) = record
            .and_then(super::ExtensionRecord::manifest)
            .and_then(|manifest| self.slot_key(manifest))
        else {
            return;
        };
        self.ensure_running(key, Deferred::Machines(MachineRequest::List(kind)), cx);
    }

    pub(in crate::environment_shell) fn run_machine_request(
        &mut self,
        key: &SlotKey,
        request: MachineRequest,
        cx: &mut Context<Self>,
    ) {
        match request {
            MachineRequest::List(kind) => self.list_machines(key, kind, cx),
            MachineRequest::Connect(env) => self.ask_for_channel(key, &env, cx),
        }
    }

    fn list_machines(&mut self, key: &SlotKey, kind: HostKindRef, cx: &mut Context<Self>) {
        let Some(connection) = self.extensions.connection(key) else {
            return;
        };
        let title = self.extensions.title_of(&key.extension);
        cx.spawn(async move |this, cx| {
            let params = ListMachinesParams {
                kind: kind.kind.clone(),
            };
            let listed = connection.request::<ListMachines>(&params).await;
            this.update(cx, |shell, cx| {
                let HostDialog::Machines(dialog) = &mut shell.hosts.dialog else {
                    return;
                };
                if dialog.kind != kind {
                    return;
                }
                match listed {
                    Ok(machines) => dialog.show(machine_choices(machines)),
                    Err(error) => dialog.fail(format!("{title} {LIST_FAILED_TEXT}: {error}")),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn create_machine_environment(
        &mut self,
        kind: &HostKindRef,
        machine: &Machine,
        cx: &mut Context<Self>,
    ) {
        self.hosts.dialog = HostDialog::Closed;
        let name = self.unused_env_name();
        let mut data = EnvironmentData::empty(name.clone());
        data.title = Some(machine.label.clone());
        data.host = Host::Extension {
            machine: ExtensionMachine {
                extension: kind.extension.to_string(),
                kind: kind.kind.to_string(),
                id: machine.id.to_string(),
                label: machine.label.clone(),
            },
        };
        self.order.push(name.clone());
        let link = self.link_for(&data.host);
        self.envs
            .insert(name.clone(), LiveEnv::new(data, None, link));
        self.switch_env(&name, cx);
    }

    pub(in crate::environment_shell) fn connect_machine(
        &mut self,
        env: &EnvironmentName,
        attempt: Attempt,
        cx: &mut Context<Self>,
    ) {
        let Some(machine) = self.machine_of(env) else {
            return;
        };
        if !self.extensions.loaded {
            self.extensions
                .machines
                .pending
                .insert(env.clone(), (Pending::WaitingForExtensions(attempt), None));
            return;
        }
        let extension = machine.extension.parse::<ExtensionId>().ok();
        let record = extension.as_ref().and_then(|id| self.extensions.record(id));
        let kind = machine.kind.parse::<HostKindId>().ok();
        let reach = kind
            .as_ref()
            .map_or(Reach::NoHostKind, |kind| reach_of(record, kind));
        let title = record.map_or_else(|| machine.extension.clone(), super::ExtensionRecord::title);
        if reach != Reach::Reachable {
            self.machine_failed(env, reach.explain(&title));
            return;
        }
        let Some(key) = record
            .and_then(super::ExtensionRecord::manifest)
            .and_then(|manifest| self.slot_key(manifest))
        else {
            return;
        };
        if let Some(Slot::Failed(reason)) = self.extensions.slots.get(&key) {
            let reason = reason.clone();
            self.machine_failed(env, format!("{title}: {reason}"));
            return;
        }
        self.extensions
            .machines
            .pending
            .insert(env.clone(), (Pending::Asking(attempt), Some(key.clone())));
        self.ensure_running(
            key,
            Deferred::Machines(MachineRequest::Connect(env.clone())),
            cx,
        );
    }

    fn machine_of(&self, env: &EnvironmentName) -> Option<ExtensionMachine> {
        match &self.envs.get(env)?.data.host {
            Host::Extension { machine } => Some(machine.clone()),
            Host::Local | Host::Ssh { .. } => None,
        }
    }

    fn machine_failed(&mut self, env: &EnvironmentName, reason: String) {
        self.extensions.machines.pending.remove(env);
        self.set_host(env, HostState::Failed(reason));
    }

    fn ask_for_channel(&mut self, key: &SlotKey, env: &EnvironmentName, cx: &mut Context<Self>) {
        let Some(machine) = self.machine_of(env) else {
            return;
        };
        let Some(connection) = self.extensions.connection(key) else {
            return;
        };
        let (Ok(kind), Ok(id)) = (
            machine.kind.parse::<HostKindId>(),
            machine.id.parse::<MachineId>(),
        ) else {
            self.machine_failed(env, Reach::NoHostKind.explain(&machine.extension));
            return;
        };
        let title = self.extensions.title_of(&key.extension);
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let params = ConnectMachineParams { kind, machine: id };
            let answered = connection.request::<ConnectMachine>(&params).await;
            this.update(cx, |shell, cx| {
                shell.channel_answered(&env, &title, &machine.label, answered, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn channel_answered(
        &mut self,
        env: &EnvironmentName,
        title: &str,
        label: &str,
        answered: Result<
            krad_extensions::api::protocol::MachineChannel,
            krad_extensions::program::ProgramError,
        >,
        cx: &mut Context<Self>,
    ) {
        let Some((Pending::Asking(attempt), _)) = self.extensions.machines.pending.remove(env)
        else {
            return;
        };
        let channel = match answered {
            Ok(channel) => channel,
            Err(error) => {
                self.machine_failed(
                    env,
                    format!("{title} {CONNECT_FAILED_TEXT} {label}: {error}"),
                );
                cx.notify();
                return;
            }
        };
        let Some(HostLink::Remote(remote)) = self.envs.get(env).map(|live| live.link.clone())
        else {
            return;
        };
        remote.set_machine_channel(machine_channel(channel.command));
        self.connect_link(env, attempt, cx);
    }

    pub(in crate::environment_shell) fn service_machine_connections(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let waiting: Vec<(EnvironmentName, Attempt)> = self
            .extensions
            .machines
            .pending
            .iter()
            .filter_map(|(env, (pending, _))| match pending {
                Pending::WaitingForExtensions(attempt) if self.extensions.loaded => {
                    Some((env.clone(), *attempt))
                }
                Pending::WaitingForExtensions(_) | Pending::Asking(_) => None,
            })
            .collect();
        for (env, attempt) in waiting {
            self.extensions.machines.pending.remove(&env);
            self.connect_machine(&env, attempt, cx);
        }
        let lost: Vec<(EnvironmentName, String)> = self
            .extensions
            .machines
            .pending
            .iter()
            .filter_map(|(env, (pending, key))| {
                let key = key.as_ref()?;
                if !matches!(pending, Pending::Asking(_)) {
                    return None;
                }
                let title = self.extensions.title_of(&key.extension);
                match self.extensions.slots.get(key) {
                    Some(Slot::Failed(reason)) => Some((env.clone(), format!("{title}: {reason}"))),
                    Some(Slot::Unsupported) | None => {
                        Some((env.clone(), format!("{title} {STOPPED_TEXT}")))
                    }
                    Some(Slot::Running(_) | Slot::Starting(_) | Slot::WaitingForTools(_)) => None,
                }
            })
            .collect();
        for (env, reason) in lost {
            self.machine_failed(&env, reason);
            cx.notify();
        }
    }
}
