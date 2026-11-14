use gpui_kit::{App, Context};
use krad_domain::{AgentPreset, EnvironmentName, TerminalId, ToastKind};
use krad_protocol::{EventKind, HostState, HostsSnapshot, LoginEnv, RemoteBusInfo, RemoteBusState};
use std::path::PathBuf;

use super::super::activity::{TerminalsTab, UtilityTab};
use super::super::live_terminal::{terminal_ref, LiveTerminal};
use super::super::runtime_view::HostMove;
use super::super::EnvironmentShell;

const LOGIN_ENV_MISSING: &str =
    "Could not read the login environment; agent CLIs set up in .bashrc or .zprofile may not be found on";
const SIGN_IN_LABEL: &str = "Sign in";
const LOST_CONNECTION: &str = "Lost the connection to";
const RECONNECTING_SUFFIX: &str = "reconnecting";
const BUS_UNAVAILABLE: &str = "Agents on this host run without the agent bus";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn host_event_heard(
        &mut self,
        kind: EventKind,
        cx: &mut Context<Self>,
    ) {
        match kind {
            EventKind::SignInWanted { environment } => self.sign_in_wanted(&environment, cx),
            EventKind::MachineChannelWanted { environment } => {
                self.connect_machine(&environment, cx);
            }
            EventKind::HostAgentsFound { environment, .. } => {
                self.host_agents_found(&environment, cx);
            }
            EventKind::LocalAgentsFound(found) => self.local_agents_found(&found, cx),
            EventKind::AcpAdapterChanged(kept) => self.acp_adapter_heard(kept.agent, cx),
            _ => {}
        }
    }

    pub(in crate::environment_shell) fn hosts_replaced(
        &mut self,
        snapshot: &HostsSnapshot,
        moves: Vec<HostMove>,
        cx: &mut Context<Self>,
    ) {
        for found in &snapshot.local {
            self.local_agents_found(found, cx);
        }
        for moved in moves {
            self.host_moved(&moved, cx);
        }
    }

    pub(in crate::environment_shell) fn host_moved(
        &mut self,
        moved: &HostMove,
        cx: &mut Context<Self>,
    ) {
        let env = &moved.environment;
        if let Some(terminal) = moved.before.sign_in_terminal() {
            let finished = moved.after.sign_in_terminal().is_none()
                && !matches!(moved.after, HostState::Failed { .. });
            if finished {
                self.drop_sign_in_terminal(env, terminal);
            }
        }
        if moved.after.is_connected() && !moved.before.is_connected() {
            self.host_connected(env, cx);
        }
        let lost = matches!(
            moved.after,
            HostState::Reconnecting { .. } | HostState::Disconnected
        );
        if moved.before.is_connected() && lost {
            if let Some(live) = self.envs.get(env) {
                let destination = live.data.host.to_string();
                self.push_toast(
                    ToastKind::Info,
                    format!("{LOST_CONNECTION} {destination}; {RECONNECTING_SUFFIX}"),
                );
            }
        }
        cx.notify();
    }

    fn drop_sign_in_terminal(&mut self, env: &EnvironmentName, id: TerminalId) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        live.sessions.remove(&id);
        live.data.side_panel_mut().remove_terminal(id);
        if live.focused == Some(id) {
            live.focused = live.shown_tab();
        }
    }

    fn host_connected(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        live.sessions.retain(|_, session| session.is_running());
        let roots: Vec<PathBuf> = live
            .data
            .groups
            .iter()
            .map(|group| group.root.clone())
            .collect();
        let host = live.data.host.clone();
        self.forget_review_offers(&host, &roots);
        self.tell_missing_login_env(env, cx);
        self.finish_host_connection(env, cx);
    }

    fn tell_missing_login_env(&mut self, env: &EnvironmentName, cx: &App) {
        let Some(host) = self.envs.get(env).map(|live| live.data.host.clone()) else {
            return;
        };
        let missing = matches!(
            self.host_state(env, cx),
            HostState::Connected {
                login_env: LoginEnv::Missing,
                ..
            }
        );
        if !missing {
            return;
        }
        let destination = host.to_string();
        if !self.hosts.login_env_missing_told.insert(host) {
            return;
        }
        self.push_toast(
            ToastKind::Error,
            format!("{LOGIN_ENV_MISSING} {destination}"),
        );
    }

    fn finish_host_connection(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        if !self.host_ready(env, cx) {
            return;
        }
        if *env == self.active {
            self.start_system_terminals(cx);
            self.refresh_git(cx);
            self.forget_file_index();
            self.reload_file_tree(cx);
            self.refresh_vault(cx);
        }
        self.store();
    }

    fn sign_in_wanted(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        let Some(id) = self.alloc_terminal_id(env) else {
            return;
        };
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let destination = live.data.host.to_string();
        live.data
            .side_panel_mut()
            .add_helper_terminal(id, format!("{SIGN_IN_LABEL} · {destination}"));
        live.terminals_tab = TerminalsTab::Shown;
        let view = self.runtime.handle.open_sign_in(terminal_ref(env, id));
        self.keep_live_session(env, id, LiveTerminal::new(view, None));
        if *env == self.active {
            self.show_utilities(UtilityTab::Terminals, cx);
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn remote_bus_moved(
        &mut self,
        env: &EnvironmentName,
        before: &RemoteBusState,
        cx: &mut Context<Self>,
    ) {
        let bus = self.runtime.view.read(cx).hosts.bus(env).clone();
        if let RemoteBusState::Unavailable { reason } = &bus {
            self.remote_bus_unavailable(env, reason);
        }
        if before.is_preparing() && !bus.is_preparing() {
            self.remote_bus_settled(env, cx);
        }
    }

    fn remote_bus_unavailable(&mut self, env: &EnvironmentName, reason: &str) {
        let Some(host) = self.envs.get(env).map(|live| live.data.host.clone()) else {
            return;
        };
        if !self.bus_work.remote_unavailable_told.insert(host) {
            return;
        }
        self.push_toast(ToastKind::Error, format!("{BUS_UNAVAILABLE}: {reason}"));
    }

    fn remote_bus_settled(&mut self, _env: &EnvironmentName, cx: &mut Context<Self>) {
        let harnesses: Vec<AgentPreset> =
            AgentPreset::listed(&self.runtime.settings.acp.all_agents()).collect();
        for harness in harnesses {
            self.flush_chats_of(harness, cx);
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn remote_bus<'a>(
        &self,
        env: &EnvironmentName,
        cx: &'a App,
    ) -> Option<&'a RemoteBusInfo> {
        self.runtime.view.read(cx).hosts.bus(env).ready()
    }

    pub(in crate::environment_shell) fn awaits_remote_bus(
        &self,
        env: &EnvironmentName,
        cx: &App,
    ) -> bool {
        self.runtime.view.read(cx).hosts.bus(env).is_preparing()
    }
}
