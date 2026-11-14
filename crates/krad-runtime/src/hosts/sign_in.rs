use krad_domain::{EnvironmentName, Host, ProcessRun};
use krad_host::HostLink;
use krad_protocol::{AgentRef, EventKind, HostState, TerminalSummary};
use std::sync::Arc;

use super::state::Attempt;
use super::{HostWork, Hosts, Waiting};
use crate::terminals::{Shared, TerminalLaunch, TerminalPlace, Terminals};

const SIGN_IN_UNAVAILABLE: &str = "This build cannot open an SSH sign-in terminal";
const NO_SIGN_IN_WAITING: &str = "No sign-in is waiting for this environment";

pub(crate) fn sign_in_launch(link: &HostLink) -> Option<TerminalLaunch> {
    let HostLink::Remote(remote) = link else {
        return None;
    };
    if remote.machine().is_some() {
        return None;
    }
    let (program, args) = remote.sign_in_terminal();
    Some(TerminalLaunch {
        program: program.to_string_lossy().into_owned(),
        args: args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect(),
        cwd: None,
        vars: Vec::new(),
        place: TerminalPlace::Here,
        harness: None,
        prefill: Vec::new(),
        typed: None,
        tap: None,
    })
}

impl Hosts {
    pub(super) fn want_sign_in(
        &mut self,
        environment: &EnvironmentName,
        host: &Host,
        outbox: &mut Vec<EventKind>,
    ) {
        if sign_in_launch(&self.links.link(host)).is_none() {
            let reason = SIGN_IN_UNAVAILABLE.to_owned();
            return self.set(environment, HostState::Failed { reason }.into(), outbox);
        }
        if let Some(entry) = self.envs.get_mut(environment) {
            entry.waiting = Waiting::SignInTerminal;
        }
        outbox.push(EventKind::SignInWanted {
            environment: environment.clone(),
        });
    }

    pub(crate) fn open_sign_in(
        &mut self,
        terminal: AgentRef,
        shared: Arc<Shared>,
        work: &mut HostWork<'_>,
    ) {
        let waiting = self.envs.get_mut(&terminal.environment).and_then(|entry| {
            matches!(entry.waiting, Waiting::SignInTerminal).then(|| {
                entry.waiting = Waiting::Nothing;
                entry.host.clone()
            })
        });
        let launch = waiting.and_then(|host| sign_in_launch(&self.links.link(&host)));
        let Some(launch) = launch else {
            return Terminals::refuse(terminal, &shared, NO_SIGN_IN_WAITING, work.outbox);
        };
        let environment = terminal.environment.clone();
        let signing_in = HostState::SigningIn {
            terminal: terminal.terminal,
        };
        work.terminals.open(terminal, launch, shared, work.outbox);
        self.set(&environment, signing_in.into(), work.outbox);
    }

    pub(crate) fn terminals_ended(&mut self, ended: &[TerminalSummary], work: &mut HostWork<'_>) {
        for summary in ended {
            let environment = &summary.terminal.environment;
            let code = match &summary.run {
                ProcessRun::Exited { code } => *code,
                ProcessRun::Running => continue,
                ProcessRun::Failed { .. } => None,
            };
            let next = self
                .envs
                .get(environment)
                .and_then(|entry| entry.tracked.after_sign_in(summary.terminal.terminal, code));
            let Some(next) = next else {
                continue;
            };
            let signed_in = next.state == HostState::SignedIn;
            self.set(environment, next, work.outbox);
            if signed_in {
                work.terminals.close(&summary.terminal);
                self.connect(environment, Attempt::AfterSignIn, work);
            }
        }
    }
}
