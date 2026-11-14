mod badge;
mod failure;
mod folder_browser;
mod look;
mod reactions;
mod sign_in;
mod ssh_dialog;

pub(super) use folder_browser::{FolderBrowser, FolderPurpose};
pub(super) use ssh_dialog::SshDialog;

use gpui_kit::{AnyElement, App, Context, Window};
use krad_domain::{EnvironmentName, Host};
use krad_extensions::host::HostLink;
use krad_protocol::{Command, HostCommand, HostState};
use std::collections::HashSet;
use std::path::PathBuf;

use super::terminal_card::home_dir;
use super::EnvironmentShell;

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
    login_env_missing_told: HashSet<Host>,
}

impl EnvironmentShell {
    pub(super) fn link_for(&self, host: &Host) -> HostLink {
        self.runtime.handle.host_link(host)
    }

    pub(in crate::environment_shell) fn host_state<'a>(
        &self,
        env: &EnvironmentName,
        cx: &'a App,
    ) -> &'a HostState {
        self.runtime.view.read(cx).hosts.state(env)
    }

    pub(super) fn host_ready(&self, env: &EnvironmentName, cx: &App) -> bool {
        self.envs.get(env).is_some_and(|live| {
            live.data.host.is_local() || self.host_state(env, cx).is_connected()
        })
    }

    pub(super) fn connect_worker_hosts(&mut self, cx: &mut Context<Self>) {
        let with_workers: Vec<EnvironmentName> = self
            .envs
            .iter()
            .filter(|(_, live)| !live.data.host.is_local() && live.data.has_open_workers())
            .map(|(name, _)| name.clone())
            .collect();
        for env in with_workers {
            self.show_host(&env, cx);
        }
    }

    pub(in crate::environment_shell) fn runs_in_background(
        &self,
        env: &EnvironmentName,
        cx: &App,
    ) -> bool {
        *env != self.active
            && self.host_state(env, cx).is_connected()
            && self
                .envs
                .get(env)
                .is_some_and(|live| live.data.has_open_workers())
    }

    pub(super) fn show_host(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        let connects = self.envs.get(env).is_some_and(|live| {
            !live.data.host.is_local() && self.host_state(env, cx).connects_when_shown()
        });
        if connects {
            self.ask_host(HostCommand::Show {
                environment: env.clone(),
            });
            cx.notify();
        }
    }

    pub(super) fn reconnect_to_host(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        if self.host_state(env, cx).can_reconnect() {
            self.ask_host(HostCommand::Reconnect {
                environment: env.clone(),
            });
            cx.notify();
        }
    }

    fn ask_host(&mut self, command: HostCommand) {
        self.save_view();
        self.runtime.client.post(Command::Host(command));
    }

    pub(super) fn service_hosts(&mut self, cx: &mut Context<Self>) {
        self.service_machine_connections(cx);
    }

    pub(super) fn close_hosts(&self) {
        self.runtime
            .client
            .post(Command::Host(HostCommand::CloseAll));
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

    pub(super) fn env_home(&self, env: &EnvironmentName, cx: &App) -> Option<PathBuf> {
        let live = self.envs.get(env)?;
        if live.data.host.is_local() {
            return home_dir().map(PathBuf::from);
        }
        match self.host_state(env, cx) {
            HostState::Connected { home, .. } => home.clone(),
            _ => None,
        }
    }
}
