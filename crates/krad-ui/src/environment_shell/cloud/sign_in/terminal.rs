use gpui_kit::{AppContext as _, Context};
use krad_domain::{CloudRunnerId, EnvironmentName, TerminalId, ToastKind};
use krad_host::{quote, Forward, HostError, HostLink};
use std::collections::HashSet;
use std::io::ErrorKind;

use super::super::super::activity::{TerminalsTab, UtilityTab};
use super::super::super::host::{remote_terminal, remote_terminal_opener};
use super::super::super::sessions::Launch;
use super::super::super::EnvironmentShell;
use super::super::{HelperTerminal, NodeKey};
use super::callback::{loopback_callbacks, LoopbackCallback};
use super::RunnerCli;

const SHELL_PROGRAM: &str = "sh";
const SHELL_FLAG: &str = "-c";
const STAY_IN_SHELL: &str = "; exec \"${SHELL:-/bin/sh}\" -l";
const ARG_SEPARATOR: &str = " ";
const TAIL_KEPT: usize = 4096;
const LABEL_PREFIX: &str = "Sign in on";
const NOT_READY: &str = "The cloud runner is not connected yet; try again once it is ready";
const NO_TERMINAL: &str = "Could not open a terminal on the cloud runner";
const FORWARDING_PREFIX: &str = "The browser can now finish the sign-in through";
const PORT_TAKEN_NOTE: &str = "is already used on this computer, so the browser cannot reach the sign-in on the runner. Free the port and open";
const FORWARD_FAILED: &str = "Could not forward the sign-in page of the runner";

pub(in crate::environment_shell) struct SignInWatch {
    runner: CloudRunnerId,
    link: HostLink,
    tail: String,
    seen: HashSet<u16>,
    forwards: Vec<Forward>,
}

impl SignInWatch {
    fn new(runner: CloudRunnerId, link: HostLink) -> Self {
        Self {
            runner,
            link,
            tail: String::new(),
            seen: HashSet::new(),
            forwards: Vec::new(),
        }
    }

    fn heard(&mut self, bytes: &[u8]) -> Vec<LoopbackCallback> {
        self.tail.push_str(&String::from_utf8_lossy(bytes));
        let fresh: Vec<LoopbackCallback> = loopback_callbacks(&self.tail)
            .into_iter()
            .filter(|callback| self.seen.insert(callback.port))
            .collect();
        if self.tail.len() > TAIL_KEPT {
            let mut cut = self.tail.len() - TAIL_KEPT;
            while !self.tail.is_char_boundary(cut) {
                cut += 1;
            }
            self.tail.drain(..cut);
        }
        fresh
    }
}

fn login_script(program: &str, args: &[&str]) -> String {
    std::iter::once(quote(program))
        .chain(args.iter().map(|arg| quote(arg)))
        .collect::<Vec<_>>()
        .join(ARG_SEPARATOR)
        + STAY_IN_SHELL
}

fn forward_failure(callback: &LoopbackCallback, error: &HostError) -> String {
    match error {
        HostError::Io(io) if io.kind() == ErrorKind::AddrInUse => {
            format!("Port {} {PORT_TAKEN_NOTE} {}", callback.port, callback.url)
        }
        other => format!("{FORWARD_FAILED} ({}): {other}", callback.url),
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_runner_sign_in(
        &mut self,
        runner: CloudRunnerId,
        cli: RunnerCli,
        cx: &mut Context<Self>,
    ) {
        let Some(link) = self.runner_link(runner) else {
            self.ensure_runner_ready(runner, cx);
            return self.toast(ToastKind::Warning, NOT_READY, cx);
        };
        let program = match (cli.harness(), cli.git_program()) {
            (Some(harness), _) => match self.launch_program(harness) {
                Ok(program) => program,
                Err(error) => return self.toast(ToastKind::Warning, &error.to_string(), cx),
            },
            (None, Some(git)) => git.to_owned(),
            (None, None) => return,
        };
        let args = vec![
            SHELL_FLAG.to_owned(),
            login_script(&program, cli.login_args()),
        ];
        let env = self.active.clone();
        let Some(id) = self.alloc_terminal_id(&env) else {
            return;
        };
        let key = (env.clone(), id);
        let given = Some((SHELL_PROGRAM, args.as_slice()));
        let launch = if let Some(opener) = remote_terminal_opener(&link, given, None, &[]) {
            self.cloud.openers.insert(key.clone(), opener);
            Launch::SignIn {
                program: SHELL_PROGRAM.to_owned(),
                args,
            }
        } else {
            match remote_terminal(&link, given, None, &[]) {
                Ok(Some((program, args))) => Launch::SignIn { program, args },
                Ok(None) => return self.toast(ToastKind::Error, NO_TERMINAL, cx),
                Err(error) => {
                    return self.toast(ToastKind::Error, &format!("{NO_TERMINAL}: {error}"), cx);
                }
            }
        };
        let label = format!(
            "{LABEL_PREFIX} {} · {}",
            self.runner_name(runner),
            cli.label()
        );
        if let Some(live) = self.envs.get_mut(&env) {
            live.data.side_panel.add_terminal(id, label);
            live.terminals_tab = TerminalsTab::Shown;
        }
        self.cloud
            .helpers
            .insert(key, HelperTerminal::SignIn(SignInWatch::new(runner, link)));
        tracing::info!(%runner, cli = cli.label(), "signing in on the cloud runner");
        self.start_session(&env, id, launch, None, &[]);
        self.show_utilities(UtilityTab::Terminals, cx);
        self.store();
        cx.notify();
    }

    pub(in crate::environment_shell) fn watch_runner_sign_in(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        bytes: &[u8],
    ) {
        let key = (env.clone(), id);
        let Some(HelperTerminal::SignIn(watch)) = self.cloud.helpers.get_mut(&key) else {
            return;
        };
        let heard = watch.heard(bytes);
        self.cloud
            .forward_asks
            .extend(heard.into_iter().map(|callback| (key.clone(), callback)));
    }

    pub(in crate::environment_shell) fn runner_helper_exited(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
    ) {
        let Some(helper) = self.cloud.helpers.get_mut(&(env.clone(), id)) else {
            return;
        };
        if let HelperTerminal::SignIn(watch) = std::mem::replace(helper, HelperTerminal::Plain) {
            self.cloud.finished.push(watch);
        }
    }

    pub(in crate::environment_shell) fn end_runner_helper(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
    ) {
        if let Some(HelperTerminal::SignIn(watch)) = self.cloud.helpers.remove(&(env.clone(), id)) {
            self.cloud.finished.push(watch);
        }
    }

    pub(in crate::environment_shell) fn runner_helper_terminals(
        &self,
        env: &EnvironmentName,
    ) -> Vec<TerminalId> {
        self.cloud
            .helpers
            .keys()
            .filter(|(owner, _)| owner == env)
            .map(|(_, id)| *id)
            .collect()
    }

    pub(in crate::environment_shell) fn service_runner_sign_ins(&mut self, cx: &mut Context<Self>) {
        for (key, callback) in std::mem::take(&mut self.cloud.forward_asks) {
            self.forward_sign_in(key, callback, cx);
        }
        for watch in std::mem::take(&mut self.cloud.finished) {
            let SignInWatch {
                runner, forwards, ..
            } = watch;
            if !forwards.is_empty() {
                cx.background_spawn(async move { drop(forwards) }).detach();
            }
            self.check_runner_logins(runner, cx);
        }
    }

    fn forward_sign_in(
        &mut self,
        key: NodeKey,
        callback: LoopbackCallback,
        cx: &mut Context<Self>,
    ) {
        let Some(HelperTerminal::SignIn(watch)) = self.cloud.helpers.get(&key) else {
            return;
        };
        let HostLink::Remote(ssh) = watch.link.clone() else {
            return;
        };
        let port = callback.port;
        let opened = cx.background_spawn(async move { ssh.forward_local_at(port) });
        cx.spawn(async move |this, cx| {
            let opened = opened.await;
            this.update(cx, |shell, cx| {
                shell.sign_in_forwarded(&key, &callback, opened, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn sign_in_forwarded(
        &mut self,
        key: &NodeKey,
        callback: &LoopbackCallback,
        opened: Result<Forward, HostError>,
        cx: &mut Context<Self>,
    ) {
        let forward = match opened {
            Ok(forward) => forward,
            Err(error) => {
                tracing::info!(port = callback.port, %error, "the sign-in callback was not forwarded");
                return self.toast(ToastKind::Warning, &forward_failure(callback, &error), cx);
            }
        };
        let Some(HelperTerminal::SignIn(watch)) = self.cloud.helpers.get_mut(key) else {
            cx.background_spawn(async move { drop(forward) }).detach();
            return;
        };
        watch.forwards.push(forward);
        let text = format!("{FORWARDING_PREFIX} {}", callback.url);
        self.toast(ToastKind::Info, &text, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_login_runs_and_leaves_a_shell_behind() {
        assert_eq!(
            login_script("codex", &["login", "--device-auth"]),
            "'codex' 'login' '--device-auth'; exec \"${SHELL:-/bin/sh}\" -l"
        );
    }

    #[test]
    fn a_callback_split_across_reads_is_found_once() {
        let mut watch = SignInWatch::new(CloudRunnerId(1), HostLink::Local);
        assert_eq!(watch.heard(b"open http://localhost:14").len(), 0);
        let heard = watch.heard(b"55/auth/callback\r\n");
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].port, 1455);
        assert_eq!(watch.heard(b"again http://localhost:1455/x\n").len(), 0);
        watch.heard(&[b'x'; TAIL_KEPT * 2]);
        assert!(watch.tail.len() <= TAIL_KEPT);
    }

    #[test]
    fn a_busy_port_shows_the_url_to_open() {
        let callback = LoopbackCallback {
            port: 1455,
            url: "http://localhost:1455/auth/callback".to_owned(),
        };
        let busy = HostError::Io(std::io::Error::from(ErrorKind::AddrInUse));
        let text = forward_failure(&callback, &busy);
        assert!(text.starts_with("Port 1455 is already used"));
        assert!(text.ends_with("http://localhost:1455/auth/callback"));
    }
}
