use krad_agents::{Caller, TerminalHandle};
use krad_domain::{CloudPlacement, CodeTools, ExtensionTools};
use krad_protocol::{EventKind, LaunchedAs, TerminalLaunched};
use krad_session::{default_shell, PtyError};
use std::io;
use std::time::Instant;

use super::super::Runtime;
use crate::cloud_runs::{
    saved_process, site_of, CloudProgram, CloudSite, CloudTerminal, NodeSite, ProcessStart,
    StartWait, REMOVED_RUNNER,
};
use crate::hosts::remote_detached_caller_vars;
use crate::launches::{AgentLine, AgentRole, LaunchSite, LineContext, WaitingLaunch};
use crate::terminals::{TerminalLaunch, TerminalPlace};

const SHELL_SUBMIT: &str = "\r";

impl Runtime {
    pub(in crate::runtime) fn launch_in_cloud(&mut self, waiting: WaitingLaunch, placement: CloudPlacement) {
        let site = match site_of(Some(placement), &self.runners) {
            NodeSite::Cloud(site) => site,
            NodeSite::Env => return,
            NodeSite::Waiting(runner) => {
                if self.settings.current.cloud.find(runner).is_none() {
                    return self.open_failed(waiting, REMOVED_RUNNER.to_owned());
                }
                self.cloud_runs.want(runner);
                return self.launches.wait_for_cloud(waiting);
            }
        };
        if self.runners.buses.is_preparing(site.placement.runner) {
            return self.launches.wait_for_cloud(waiting);
        }
        let saved = saved_process(&self.environments, &waiting.terminal);
        match self
            .cloud_runs
            .start(&waiting.terminal, &site, saved, &self.jobs)
        {
            Ok(start) => self.open_in_cloud(waiting, *site, &start),
            Err(StartWait::Waiting) => self.launches.wait_for_cloud(waiting),
            Err(StartWait::Failed(problem)) => self.open_failed(waiting, problem),
        }
    }

    pub(in crate::runtime) fn retry_cloud_launches(&mut self) {
        for waiting in self.launches.take_cloud_waiting() {
            let placement = self
                .environments
                .data(&waiting.terminal.environment)
                .filter(|data| data.nodes.iter().any(|node| node.id == waiting.terminal.terminal))
                .and_then(|_| crate::cloud_runs::placement_of(&self.environments, &waiting.terminal));
            if let Some(placement) = placement {
                self.launch_in_cloud(waiting, placement);
            }
        }
    }

    fn cloud_line(&self, waiting: &WaitingLaunch, site: &CloudSite) -> Option<AgentLine> {
        let settings = &self.settings.current;
        let remote = self.runners.buses.ready(site.placement.runner);
        let leads = self
            .environments
            .data(&waiting.terminal.environment)
            .and_then(|data| data.session_led_by(waiting.terminal.terminal))
            .is_some();
        let context = LineContext {
            settings,
            site: LaunchSite::Ssh,
            bus: self.bus.as_ref().map(crate::bus::Bus::access),
            remote,
            role: if leads {
                AgentRole::Lead
            } else {
                AgentRole::Member
            },
            tools_offered: settings.code_tools == CodeTools::On
                || settings.extension_tools == ExtensionTools::On
                || self.kraddies.offers_memory_tools(),
        };
        context.line(&waiting.intent)
    }

    fn cloud_vars(&self, waiting: &WaitingLaunch, site: &CloudSite) -> Vec<(String, String)> {
        let caller = Caller::Terminal(TerminalHandle {
            environment: waiting.terminal.environment.as_str().to_owned(),
            terminal: waiting.terminal.terminal.as_u64(),
        });
        self.runners
            .buses
            .ready(site.placement.runner)
            .map_or_else(Vec::new, |bus| remote_detached_caller_vars(bus, &caller))
    }

    #[tracing::instrument(skip_all, fields(environment = waiting.terminal.environment.as_str(), terminal = waiting.terminal.terminal.as_u64()))]
    fn open_in_cloud(&mut self, waiting: WaitingLaunch, site: CloudSite, start: &ProcessStart) {
        let harness = waiting.intent.harness();
        let line = self.cloud_line(&waiting, &site);
        let mut program = CloudProgram::login_shell();
        program.vars = self.cloud_vars(&waiting, &site);
        let (line, after_start) = line
            .filter(|_| start.fresh)
            .map_or((None, None), |(line, after)| (Some(line), after));
        let typed = line.map(|line| {
            format!("{}{line}{SHELL_SUBMIT}", LaunchSite::Ssh.clear_prefix()).into_bytes()
        });
        let launched = TerminalLaunched {
            terminal: waiting.terminal.clone(),
            harness,
            launched_as: if typed.is_some() {
                LaunchedAs::Line
            } else {
                LaunchedAs::Shell
            },
            resume: None,
        };
        let (shell, args) = default_shell();
        let launch = TerminalLaunch {
            program: shell,
            args,
            cwd: Some(site.placement.worktree.clone()),
            vars: Vec::new(),
            place: TerminalPlace::Opened(CloudTerminal::new(site, start, program).opener()),
            harness,
            prefill: waiting.prefill,
            typed,
            tap: None,
        };
        self.outbox.push(EventKind::TerminalLaunched(launched));
        let terminal = waiting.terminal;
        let shared = self.launches.view(terminal.clone());
        self.terminals
            .open(terminal.clone(), launch, shared, &mut self.outbox);
        if let Some(prompt) = after_start {
            self.typist.prompt(terminal, prompt, Instant::now());
        }
    }

    fn open_failed(&mut self, waiting: WaitingLaunch, problem: String) {
        tracing::debug!(%problem, "a cloud terminal could not open");
        let (program, args) = default_shell();
        let launch = TerminalLaunch {
            program,
            args,
            cwd: None,
            vars: Vec::new(),
            place: TerminalPlace::Opened(Box::new(move |_, _| {
                Err(PtyError::Io(io::Error::other(problem)))
            })),
            harness: waiting.intent.harness(),
            prefill: Vec::new(),
            typed: None,
            tap: None,
        };
        let shared = self.launches.view(waiting.terminal.clone());
        self.terminals
            .open(waiting.terminal, launch, shared, &mut self.outbox);
    }
}
