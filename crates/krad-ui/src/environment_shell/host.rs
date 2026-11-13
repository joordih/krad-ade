mod badge;
mod failure;
mod folder_browser;
mod look;
mod reactions;
mod sign_in;
mod ssh_dialog;
mod tools;

pub(super) use folder_browser::{FolderBrowser, FolderPurpose};
pub(super) use ssh_dialog::{ssh_config_suggestions, SshDialog};
pub use tools::HostTools;

use gpui_kit::{AnyElement, Context, Window};
use krad_domain::{EnvironmentName, Host, TerminalId};
use krad_host::{HostContext, HostLink, HOST_CLI_PROGRAM};
use krad_protocol::{Command, HostCommand, HostState};
use std::collections::HashSet;
use std::path::PathBuf;

use super::terminal_card::home_dir;
use super::EnvironmentShell;

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
    login_env_missing_told: HashSet<Host>,
}

impl EnvironmentShell {
    pub(super) fn link_for(&self, host: &Host) -> HostLink {
        self.runtime.handle.host_link(host)
    }

    pub(super) fn host_context(&self) -> HostContext {
        HostContext {
            config_dir: krad_store::config_dir(),
            install_id: self.host_tools.install_id.clone(),
            timeouts: self.settings.host_timeouts,
            transport: None,
            local_cli: self.host_tools.local_cli.clone(),
            host_clis: self.host_tools.host_clis.clone(),
            prompts: self.runtime.handle.prompt_sender(),
        }
    }

    pub(in crate::environment_shell) fn host_state(&self, env: &EnvironmentName) -> &HostState {
        self.runtime.view.hosts.state(env)
    }

    pub(super) fn host_ready(&self, env: &EnvironmentName) -> bool {
        self.envs
            .get(env)
            .is_some_and(|live| live.data.host.is_local() || self.host_state(env).is_connected())
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

    pub(in crate::environment_shell) fn runs_in_background(&self, env: &EnvironmentName) -> bool {
        *env != self.active
            && self.host_state(env).is_connected()
            && self
                .envs
                .get(env)
                .is_some_and(|live| live.data.has_open_workers())
    }

    pub(super) fn show_host(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        let connects = self.envs.get(env).is_some_and(|live| {
            !live.data.host.is_local() && self.host_state(env).connects_when_shown()
        });
        if connects {
            self.ask_host(HostCommand::Show {
                environment: env.clone(),
            });
            cx.notify();
        }
    }

    pub(super) fn reconnect_to_host(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        if self.host_state(env).can_reconnect() {
            self.ask_host(HostCommand::Reconnect {
                environment: env.clone(),
            });
            cx.notify();
        }
    }

    fn ask_host(&mut self, command: HostCommand) {
        self.store();
        self.runtime.handle.post(Command::Host(command));
    }

    pub(super) fn service_hosts(&mut self, cx: &mut Context<Self>) {
        self.service_machine_connections(cx);
    }

    pub(super) fn sign_in_terminal_of(&self, env: &EnvironmentName) -> Option<TerminalId> {
        self.host_state(env).sign_in_terminal()
    }

    pub(super) fn close_hosts(&self) {
        self.runtime.handle.close_hosts();
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
        let live = self.envs.get(env)?;
        match self.link_for(&live.data.host) {
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
