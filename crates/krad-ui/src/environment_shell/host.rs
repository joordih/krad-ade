mod backoff;
mod badge;
mod bus;
mod failure;
mod folder_browser;
mod problem;
mod remote_pty;
mod sign_in;
mod ssh_dialog;
mod state;
mod terminal;
mod tools;

pub(super) use folder_browser::{FolderBrowser, FolderPurpose};
pub(super) use ssh_dialog::SshDialog;
pub(super) use state::HostState;
pub(super) use terminal::{remote_terminal, remote_terminal_opener};
pub use tools::HostTools;

use gpui_kit::{AnyElement, AppContext as _, Context, Window};
use krad_domain::{EnvironmentName, Host, TerminalId};
use krad_host::{HostContext, HostError, HostLink, HostPlatform, HOST_CLI_PROGRAM};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::activity::{TerminalsTab, UtilityTab};
use super::agent_bus::RemoteBusSlot;
use super::sessions::Launch;
use super::terminal_card::home_dir;
use super::{live_meta, EnvironmentShell};

const SIGN_IN_LABEL: &str = "Sign in";
const SIGN_IN_UNAVAILABLE: &str = "This build cannot open an SSH sign-in terminal";
const SIGN_IN_NOT_KEPT: &str =
    "Signed in, but the connection did not stay open. Reconnect to try again.";
const LOST_CONNECTION: &str = "Lost the connection to";
const RECONNECTING_SUFFIX: &str = "reconnecting";
const THREAD_HOST_CLOSE: &str = "krad-ade-host-close";

#[derive(Default)]
pub(super) enum HostDialog {
    #[default]
    Closed,
    NewEnvironment(SshDialog),
    Folder(FolderBrowser),
    SignIn(sign_in::SignInDialog),
    Machines(super::extensions::MachineDialog),
}

#[derive(Default)]
pub(super) struct HostUi {
    pub(super) dialog: HostDialog,
    prompts: sign_in::PromptInbox,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum Attempt {
    First,
    AfterSignIn,
    Retry(u32),
}

impl EnvironmentShell {
    pub(super) fn link_for(&self, host: &Host) -> HostLink {
        self.envs
            .values()
            .find(|live| live.data.host == *host)
            .map_or_else(
                || HostLink::new(host, &self.host_context()),
                |live| live.link.clone(),
            )
    }

    fn host_context(&self) -> HostContext {
        HostContext {
            config_dir: krad_store::config_dir(),
            install_id: self.host_tools.install_id.clone(),
            timeouts: self.settings.host_timeouts,
            transport: None,
            local_cli: self.host_tools.local_cli.clone(),
            host_clis: self.host_tools.host_clis.clone(),
            prompts: Some(self.hosts.prompts.sender()),
        }
    }

    pub(super) fn host_ready(&self, env: &EnvironmentName) -> bool {
        self.envs
            .get(env)
            .is_some_and(|live| live.link.is_local() || live.host.is_connected())
    }

    pub(super) fn show_host(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        let connects = self
            .envs
            .get(env)
            .is_some_and(|live| !live.link.is_local() && live.host.connects_when_shown());
        if connects {
            self.connect_to_host(env, Attempt::First, cx);
        }
    }

    pub(super) fn reconnect_to_host(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        if self
            .envs
            .get(env)
            .is_some_and(|live| live.host.can_reconnect())
        {
            self.connect_to_host(env, Attempt::First, cx);
        }
    }

    fn connect_to_host(&mut self, env: &EnvironmentName, attempt: Attempt, cx: &mut Context<Self>) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let HostLink::Remote(remote) = live.link.clone() else {
            return;
        };
        live.host = HostState::Connecting;
        cx.notify();
        if remote.machine().is_some() {
            self.connect_machine(env, attempt, cx);
            return;
        }
        self.connect_link(env, attempt, cx);
    }

    pub(in crate::environment_shell) fn connect_link(
        &mut self,
        env: &EnvironmentName,
        attempt: Attempt,
        cx: &mut Context<Self>,
    ) {
        let Some(HostLink::Remote(ssh)) = self.envs.get(env).map(|live| live.link.clone()) else {
            return;
        };
        let connect = cx.background_spawn(async move { ssh.connect() });
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let outcome = connect.await;
            this.update(cx, |shell, cx| {
                shell.host_answered(&env, attempt, outcome, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn host_answered(
        &mut self,
        env: &EnvironmentName,
        attempt: Attempt,
        outcome: Result<HostPlatform, HostError>,
        cx: &mut Context<Self>,
    ) {
        let error = match outcome {
            Ok(platform) => {
                self.host_connected(env, platform, cx);
                cx.notify();
                return;
            }
            Err(error) => error,
        };
        tracing::info!(env = env.as_str(), error = %error, "host connection failed");
        let next = match attempt {
            Attempt::First if error.needs_sign_in() => {
                self.open_sign_in(env, cx);
                cx.notify();
                return;
            }
            Attempt::AfterSignIn if error.needs_sign_in() => {
                HostState::Failed(SIGN_IN_NOT_KEPT.to_owned())
            }
            Attempt::Retry(done) if problem::recovery(&error) == problem::Recovery::RetryAlone => {
                HostState::retry(done + 1, Instant::now())
                    .unwrap_or_else(|| HostState::Failed(self.explain_failure(env, &error)))
            }
            _ => HostState::Failed(self.explain_failure(env, &error)),
        };
        self.set_host(env, next);
        cx.notify();
    }

    fn explain_failure(&self, env: &EnvironmentName, error: &HostError) -> String {
        self.envs
            .get(env)
            .and_then(|live| live.data.host.destination())
            .map_or_else(
                || error.to_string(),
                |destination| problem::explain(error, destination),
            )
    }

    pub(in crate::environment_shell) fn set_host(
        &mut self,
        env: &EnvironmentName,
        state: HostState,
    ) {
        if let Some(live) = self.envs.get_mut(env) {
            live.host = state;
        }
    }

    fn host_connected(
        &mut self,
        env: &EnvironmentName,
        platform: HostPlatform,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        live.host = HostState::Connected(platform);
        live.data.meta = live_meta(live);
        live.sessions
            .retain(|_, session| session.state.is_running());
        let roots: Vec<PathBuf> = live
            .data
            .groups
            .iter()
            .map(|group| group.root.clone())
            .collect();
        live.remote_bus = RemoteBusSlot::Absent;
        let host = live.data.host.clone();
        let shared = self.envs.iter().any(|(name, other)| {
            name != env && other.data.host == host && other.host.is_connected()
        });
        self.forget_review_offers(&host, &roots);
        if !shared {
            self.opencode.hosts.remove(&host);
        }
        self.detect_host_agents(env, cx);
        self.prepare_remote_bus(env, cx);
        self.finish_host_connection(env, cx);
    }

    fn finish_host_connection(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        if !self.host_ready(env) {
            return;
        }
        self.spawn_missing_sessions(env);
        if *env == self.active {
            self.start_system_terminals();
            self.refresh_git(cx);
            self.forget_file_index();
            self.reload_file_tree(cx);
            self.refresh_vault(cx);
        }
        self.store();
    }

    fn open_sign_in(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        let sign_in = self.envs.get(env).and_then(|live| match &live.link {
            HostLink::Remote(ssh) if ssh.machine().is_none() => {
                Some((ssh.sign_in_terminal(), live.data.host.to_string()))
            }
            HostLink::Remote(_) | HostLink::Local => None,
        });
        let Some(((program, args), destination)) = sign_in else {
            self.set_host(env, HostState::Failed(SIGN_IN_UNAVAILABLE.to_owned()));
            return;
        };
        let Some(id) = self.alloc_terminal_id(env) else {
            return;
        };
        if let Some(live) = self.envs.get_mut(env) {
            live.data
                .side_panel
                .add_terminal(id, format!("{SIGN_IN_LABEL} · {destination}"));
            live.terminals_tab = TerminalsTab::Shown;
            live.host = HostState::SigningIn { terminal: id };
        }
        let launch = Launch::SignIn {
            program: program.to_string_lossy().into_owned(),
            args: args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
        };
        self.start_session(env, id, launch, None, &[]);
        if *env == self.active {
            self.show_utilities(UtilityTab::Terminals, cx);
        }
    }

    pub(super) fn sign_in_ended(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        code: Option<i32>,
    ) -> bool {
        let Some(live) = self.envs.get_mut(env) else {
            return false;
        };
        let Some(next) = live.host.after_sign_in(id, code) else {
            return false;
        };
        if next == HostState::SignedIn {
            if let Some(session) = live.sessions.remove(&id) {
                session.cancelled.store(true, Ordering::SeqCst);
            }
            live.data.side_panel.remove_terminal(id);
            if live.focused == Some(id) {
                live.focused = live.shown_tab();
            }
        }
        live.host = next;
        true
    }

    pub(super) fn service_hosts(&mut self, cx: &mut Context<Self>) {
        self.collect_sign_in_prompts(cx);
        self.service_machine_connections(cx);
        let signed_in: Vec<EnvironmentName> = self
            .envs
            .iter()
            .filter(|(_, live)| live.host == HostState::SignedIn)
            .map(|(name, _)| name.clone())
            .collect();
        for env in signed_in {
            self.connect_to_host(&env, Attempt::AfterSignIn, cx);
        }
        let now = Instant::now();
        let due: Vec<(EnvironmentName, u32)> = self
            .envs
            .iter()
            .filter_map(|(name, live)| Some((name.clone(), live.host.retry_due(now)?)))
            .collect();
        for (env, attempt) in due {
            self.connect_to_host(&env, Attempt::Retry(attempt), cx);
        }
        let dropped: Vec<(EnvironmentName, String)> = self
            .envs
            .iter_mut()
            .filter(|(_, live)| !live.link.is_ready())
            .filter_map(|(name, live)| {
                let next = live.host.dropped(now)?;
                live.host = next;
                live.remote_bus = RemoteBusSlot::Absent;
                Some((name.clone(), live.data.host.to_string()))
            })
            .collect();
        for live in self.envs.values_mut() {
            if let RemoteBusSlot::Ready(bus) = &mut live.remote_bus {
                if !bus.is_alive() {
                    live.remote_bus = RemoteBusSlot::Absent;
                }
            }
        }
        for (_, destination) in &dropped {
            self.push_toast(format!(
                "{LOST_CONNECTION} {destination}; {RECONNECTING_SUFFIX}"
            ));
        }
        if !dropped.is_empty() {
            cx.notify();
        }
    }

    pub(super) fn sign_in_terminal_of(&self, env: &EnvironmentName) -> Option<TerminalId> {
        self.envs.get(env)?.host.sign_in_terminal()
    }

    pub(super) fn close_hosts(&self) {
        let mut closed: Vec<&Host> = Vec::new();
        for live in self.envs.values() {
            if !closed.contains(&&live.data.host) {
                closed.push(&live.data.host);
                close_link(&live.link);
            }
        }
    }

    pub(super) fn host_dialog_sheet(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.show_waiting_prompt(window, cx);
        match &self.hosts.dialog {
            HostDialog::Closed => None,
            HostDialog::NewEnvironment(dialog) => Some(Self::ssh_dialog_sheet(dialog, cx)),
            HostDialog::Folder(browser) => Some(Self::folder_browser_sheet(browser, cx)),
            HostDialog::SignIn(dialog) => Self::sign_in_sheet(dialog, cx),
            HostDialog::Machines(dialog) => Some(Self::machine_dialog_sheet(dialog, cx)),
        }
    }

    pub(super) fn env_home(&self, env: &EnvironmentName) -> Option<PathBuf> {
        match &self.envs.get(env)?.link {
            HostLink::Local => home_dir().map(PathBuf::from),
            HostLink::Remote(ssh) => ssh.home().map(PathBuf::from),
        }
    }
}

pub(super) fn close_link(link: &HostLink) {
    let HostLink::Remote(ssh) = link.clone() else {
        return;
    };
    let closing = std::thread::Builder::new()
        .name(THREAD_HOST_CLOSE.to_owned())
        .spawn(move || ssh.close());
    drop(closing);
}

pub(super) fn local_cli() -> Option<PathBuf> {
    let file = format!("{HOST_CLI_PROGRAM}{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|dir| dir.join(file))
        .filter(|path| path.is_file())
}
