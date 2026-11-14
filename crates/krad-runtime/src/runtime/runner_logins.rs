use krad_domain::{AgentPreset, ClaudeOauthToken, CloudRunnerId, RunnerCli, RunnerLogin};
use krad_host::{Forward, HostError, HostLink};
use krad_protocol::{AgentRef, EventKind, RunnerNews, RunnerNotice};
use krad_session::find_program;
use std::path::Path;
use std::sync::Arc;

use super::Runtime;
use crate::input::RunnerInput;
use crate::runners::{
    forward_failure, git_program, login_args, login_script, probe_login, LoopbackCallback,
    RunnerDone, SignInWatch,
};
use crate::terminals::{HostShell, Shared, TerminalLaunch, TerminalPlace, Terminals};

const SHELL_PROGRAM: &str = "sh";
const SHELL_FLAG: &str = "-c";
const SETUP_TOKEN_ARG: &str = "setup-token";
const NOT_READY: &str = "The cloud runner is not connected yet; try again once it is ready";
const NO_PROGRAM: &str = "has no program set in the settings";
const NO_LOCAL_CLAUDE: &str = "Claude Code was not found on this computer";
const NOT_STARTED: &str = "could not start";

impl Runtime {
    pub(super) fn apply_runner_input(&mut self, input: RunnerInput) {
        match input {
            RunnerInput::OpenSignIn {
                terminal,
                runner,
                cli,
                shared,
            } => self.open_runner_sign_in((terminal, shared), runner, cli),
            RunnerInput::OpenClaudeSetup { terminal, shared } => {
                self.open_claude_setup(terminal, shared);
            }
            RunnerInput::OpenSshSignIn {
                terminal,
                runner,
                shared,
            } => self.open_runner_ssh_sign_in((terminal, shared), runner),
            RunnerInput::Connect { runner, reply } => self.open_runner_line(runner, reply),
        }
    }

    fn launch_program(&self, harness: AgentPreset) -> Option<String> {
        self.settings
            .current
            .launch_line(harness)
            .and_then(|line| line.split_whitespace().next().map(str::to_owned))
    }

    fn show_logins(&mut self, runner: CloudRunnerId) {
        let shown = self.runners.logins_shown(runner);
        self.outbox.push(EventKind::RunnerLoginsChanged(shown));
    }

    fn tell_runner(&mut self, runner: CloudRunnerId, news: RunnerNews) {
        self.outbox
            .push(EventKind::RunnerNews(RunnerNotice { runner, news }));
    }

    #[tracing::instrument(skip_all, fields(%runner))]
    pub(super) fn check_runner_logins(&mut self, runner: CloudRunnerId) {
        let Some(link) = self.runners.directory.get(runner).map(|ready| ready.link) else {
            return;
        };
        let HostLink::Remote(ssh) = &link else {
            return;
        };
        let Some(home) = ssh.home().map(Path::to_path_buf) else {
            return;
        };
        for cli in RunnerCli::ALL {
            if !self.runners.logins.begin(runner, cli) {
                continue;
            }
            let program = cli
                .harness()
                .and_then(|harness| self.launch_program(harness));
            let link = link.clone();
            let home = home.clone();
            let started = self.jobs.run(move || {
                let login = probe_login(&link, &home, cli, program);
                RunnerDone::LoginProbed { runner, cli, login }.into()
            });
            if let Err(error) = started {
                tracing::warn!(%error, "a runner sign-in check could not start");
                self.runners.logins.set(runner, cli, RunnerLogin::Unknown);
            }
        }
        self.show_logins(runner);
    }

    pub(super) fn login_probed(
        &mut self,
        runner: CloudRunnerId,
        cli: RunnerCli,
        login: RunnerLogin,
    ) {
        if self.runners.directory.get(runner).is_none() {
            return;
        }
        self.runners.logins.set(runner, cli, login);
        self.show_logins(runner);
    }

    pub(super) fn forget_runner_logins(&mut self, runner: CloudRunnerId) {
        self.runners.logins.forget(runner);
    }

    pub(super) fn save_claude_token(
        &mut self,
        runner: CloudRunnerId,
        token: Option<ClaudeOauthToken>,
    ) {
        let secrets = self.secrets.clone();
        let keeps = token.is_some();
        let started = self.jobs.run(move || {
            let saved = match &token {
                Some(token) => secrets.write_cloud_claude_token(runner, token),
                None => secrets.delete_cloud_claude_token(runner),
            }
            .map_err(|error| error.to_string());
            RunnerDone::ClaudeTokenSaved {
                runner,
                token,
                saved,
            }
            .into()
        });
        if let Err(error) = started {
            let error = format!("{NOT_STARTED}: {error}");
            let news = if keeps {
                RunnerNews::ClaudeTokenNotKept { error }
            } else {
                RunnerNews::ClaudeTokenNotForgotten { error }
            };
            self.tell_runner(runner, news);
        }
    }

    pub(super) fn claude_token_saved(
        &mut self,
        runner: CloudRunnerId,
        token: Option<ClaudeOauthToken>,
        saved: Result<(), String>,
    ) {
        let keeps = token.is_some();
        let news = match (saved, keeps) {
            (Err(error), true) => RunnerNews::ClaudeTokenNotKept { error },
            (Err(error), false) => RunnerNews::ClaudeTokenNotForgotten { error },
            (Ok(()), true) => RunnerNews::ClaudeTokenKept,
            (Ok(()), false) => RunnerNews::ClaudeTokenForgotten,
        };
        if matches!(
            news,
            RunnerNews::ClaudeTokenKept | RunnerNews::ClaudeTokenForgotten
        ) {
            self.runners.directory.keep_claude_token(runner, token);
            self.show_logins(runner);
        }
        self.tell_runner(runner, news);
    }

    #[tracing::instrument(skip_all, fields(%runner, ?cli))]
    pub(super) fn open_runner_sign_in(
        &mut self,
        (terminal, shared): (AgentRef, Arc<Shared>),
        runner: CloudRunnerId,
        cli: RunnerCli,
    ) {
        let Some(link) = self.runners.directory.get(runner).map(|ready| ready.link) else {
            return Terminals::refuse(terminal, &shared, NOT_READY, &mut self.outbox);
        };
        let program = match (cli.harness(), git_program(cli)) {
            (Some(harness), _) => {
                let Some(program) = self.launch_program(harness) else {
                    let name = self.settings.current.acp.harness_name(harness);
                    let error = format!("{name} {NO_PROGRAM}");
                    return Terminals::refuse(terminal, &shared, &error, &mut self.outbox);
                };
                program
            }
            (None, Some(git)) => git.to_owned(),
            (None, None) => return,
        };
        let mut watch = SignInWatch::new(runner, link.clone());
        let launch = TerminalLaunch {
            program: SHELL_PROGRAM.to_owned(),
            args: vec![
                SHELL_FLAG.to_owned(),
                login_script(&program, login_args(cli)),
            ],
            cwd: None,
            vars: Vec::new(),
            place: TerminalPlace::Host {
                link,
                shell: HostShell::Given,
                in_wsl: false,
            },
            harness: None,
            prefill: Vec::new(),
            typed: None,
            tap: Some(watch.tap()),
        };
        tracing::info!("signing in on the cloud runner");
        self.runners.sign_ins.watch(terminal.clone(), watch);
        self.terminals
            .open(terminal, launch, shared, &mut self.outbox);
    }

    pub(super) fn open_claude_setup(&mut self, terminal: AgentRef, shared: Arc<Shared>) {
        let program = self
            .launch_program(AgentPreset::ClaudeCode)
            .and_then(|program| find_program(&program));
        let Some(program) = program else {
            return Terminals::refuse(terminal, &shared, NO_LOCAL_CLAUDE, &mut self.outbox);
        };
        let launch = TerminalLaunch {
            program: program.to_string_lossy().into_owned(),
            args: vec![SETUP_TOKEN_ARG.to_owned()],
            cwd: None,
            vars: Vec::new(),
            place: TerminalPlace::Here,
            harness: None,
            prefill: Vec::new(),
            typed: None,
            tap: None,
        };
        self.terminals
            .open(terminal, launch, shared, &mut self.outbox);
    }

    pub(super) fn hear_runner_sign_ins(&mut self) {
        for (terminal, link, callback) in self.runners.sign_ins.heard() {
            let HostLink::Remote(ssh) = link else {
                continue;
            };
            let port = callback.port;
            let started = self.jobs.run(move || {
                let opened = ssh.forward_local_at(port);
                RunnerDone::SignInForwarded {
                    terminal,
                    callback,
                    opened,
                }
                .into()
            });
            if let Err(error) = started {
                tracing::warn!(%error, "a sign-in callback forward could not start");
            }
        }
        let terminals = &self.terminals;
        let finished = self
            .runners
            .sign_ins
            .finished(|terminal| terminals.runs(terminal));
        for watch in finished {
            let runner = watch.runner;
            self.drop_forwards(watch.forwards);
            self.check_runner_logins(runner);
        }
    }

    fn drop_forwards(&self, forwards: Vec<Forward>) {
        if forwards.is_empty() {
            return;
        }
        if let Err(error) = self.jobs.run_reporting(move |_| drop(forwards)) {
            tracing::warn!(%error, "the sign-in forwards could not be closed");
        }
    }

    pub(super) fn sign_in_forwarded(
        &mut self,
        terminal: &AgentRef,
        callback: LoopbackCallback,
        opened: Result<Forward, HostError>,
    ) {
        let forward = match opened {
            Ok(forward) => forward,
            Err(error) => {
                tracing::info!(port = callback.port, %error, "the sign-in callback was not forwarded");
                let runner = self
                    .runners
                    .sign_ins
                    .get_mut(terminal)
                    .map(|watch| watch.runner);
                let message = forward_failure(&callback, &error);
                if let Some(runner) = runner {
                    self.tell_runner(runner, RunnerNews::SignInNotForwarded { message });
                }
                return;
            }
        };
        let Some(watch) = self.runners.sign_ins.get_mut(terminal) else {
            return self.drop_forwards(vec![forward]);
        };
        watch.forwards.push(forward);
        let runner = watch.runner;
        self.tell_runner(runner, RunnerNews::SignInForwarded { url: callback.url });
    }
}
