use krad_agents::{Caller, TerminalHandle};
use krad_chat::{new_opencode_session_id, prepare_opencode_session, OpencodeServer};
use krad_domain::{
    attach_line, parse_wsl_path, AgentPreset, CodeTools, EnvironmentName, ExtensionTools, Host,
    LaunchIntent, ModelId, ResumeId,
};
use krad_host::HostLink;
use krad_protocol::{
    AgentRef, ErrorCode, EventKind, HostState, LaunchedAs, Reply, TerminalCommand,
    TerminalLaunched, WriteBy,
};
use krad_session::{default_shell, wsl_shell_in};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use super::{refused, Runtime};
use crate::cloud_runs::placement_of;
use crate::hosts::{remote_caller_vars, Reach};
use crate::jobs::JobDone;
use crate::launches::{
    local_caller_vars, AgentLine, AgentRole, LaunchSite, LineContext, WaitingLaunch,
};
use crate::terminals::{HostShell, TerminalLaunch, TerminalPlace};

const UNKNOWN_ENVIRONMENT: &str = "this runtime has no environment of that name";
const SHELL_SUBMIT: &str = "\r";

#[derive(Debug)]
pub(crate) struct AttachedLine {
    terminal: AgentRef,
    bytes: Vec<u8>,
}

struct Attached {
    server: Arc<OpencodeServer>,
    resume: ResumeId,
    cwd: PathBuf,
    model: Option<ModelId>,
}

struct Resolved {
    launch: TerminalLaunch,
    launched: TerminalLaunched,
    attached: Option<(Attached, String)>,
    after_start: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Gate {
    Open,
    Wait,
}

fn gate(reach: &Reach<'_>, intent: &LaunchIntent) -> Gate {
    match reach {
        Reach::Ready(_) => Gate::Open,
        Reach::BusPreparing if intent.harness().is_none() => Gate::Open,
        Reach::BusPreparing | Reach::Unreached => Gate::Wait,
    }
}

fn typed(site: LaunchSite, line: &str) -> Vec<u8> {
    format!("{}{line}{SHELL_SUBMIT}", site.clear_prefix()).into_bytes()
}

impl Runtime {
    pub(super) fn execute_terminal(&mut self, command: TerminalCommand) -> Option<Reply> {
        match command {
            TerminalCommand::Open {
                terminal,
                intent,
                cwd,
            } => {
                if !self.environments.knows(&terminal.environment) {
                    return Some(refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT));
                }
                self.launch(WaitingLaunch {
                    terminal,
                    intent,
                    cwd,
                    prefill: Vec::new(),
                });
                Some(Reply::Accepted)
            }
            TerminalCommand::OpenMissing { environment } => {
                if !self.environments.knows(&environment) {
                    return Some(refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT));
                }
                self.open_missing(&environment);
                Some(Reply::Accepted)
            }
            TerminalCommand::Type { terminal, typing } => Some(self.type_into(terminal, typing)),
            TerminalCommand::Close { terminal } => {
                self.typist.forget(&terminal);
                let waited = self.launches.is_waiting(&terminal);
                self.launches.forget(&terminal);
                let reply = self
                    .terminals
                    .execute(TerminalCommand::Close { terminal }, &mut self.outbox)?;
                Some(if waited { Reply::Accepted } else { reply })
            }
            command => self.terminals.execute(command, &mut self.outbox),
        }
    }

    pub(super) fn follow_connections(&mut self, unread: usize) {
        let connected: Vec<EnvironmentName> = self
            .outbox
            .iter()
            .skip(unread)
            .filter_map(|event| match event {
                EventKind::HostChanged {
                    environment,
                    state: HostState::Connected { .. },
                } => Some(environment.clone()),
                _ => None,
            })
            .collect();
        for environment in connected {
            self.forget_unshared_opencode(&environment);
            self.detect_host_agents(&environment);
            self.open_missing(&environment);
        }
        if !self.launches.has_waiting() {
            return;
        }
        for waiting in self.launches.take_waiting() {
            let kept = self
                .environments
                .data(&waiting.terminal.environment)
                .is_some_and(|data| {
                    data.nodes
                        .iter()
                        .any(|node| node.id == waiting.terminal.terminal)
                });
            if kept {
                self.launch(waiting);
            }
        }
    }

    pub(super) fn open_missing(&mut self, environment: &EnvironmentName) -> usize {
        let Some(data) = self.environments.data(environment) else {
            return 0;
        };
        if matches!(self.hosts.reach(environment, &data.host), Reach::Unreached) {
            return 0;
        }
        let terminals = &self.terminals;
        let launches = &self.launches;
        let relaunches = data.relaunches(|id| {
            let terminal = AgentRef {
                environment: environment.clone(),
                terminal: id,
            };
            terminals.runs(&terminal) || launches.is_waiting(&terminal)
        });
        let count = relaunches.len();
        for relaunch in relaunches {
            self.launch(WaitingLaunch {
                terminal: AgentRef {
                    environment: environment.clone(),
                    terminal: relaunch.id,
                },
                intent: relaunch.intent.into(),
                cwd: relaunch.cwd,
                prefill: relaunch.prefill,
            });
        }
        count
    }

    fn launch(&mut self, waiting: WaitingLaunch) {
        if let Some(placement) = placement_of(&self.environments, &waiting.terminal) {
            return self.launch_in_cloud(waiting, placement);
        }
        let Some(host) = self.environments.host_of(&waiting.terminal.environment) else {
            return;
        };
        let reach = self.hosts.reach(&waiting.terminal.environment, &host);
        if gate(&reach, &waiting.intent) == Gate::Wait {
            self.launches.wait(waiting);
            return;
        }
        let remote = match reach {
            Reach::Ready(remote) => remote.cloned(),
            Reach::BusPreparing | Reach::Unreached => None,
        };
        let Some(resolved) = self.resolve(&waiting, &host, remote.as_ref()) else {
            return;
        };
        self.open_resolved(waiting.terminal, resolved);
    }

    #[tracing::instrument(skip_all, fields(environment = terminal.environment.as_str(), terminal = terminal.terminal.as_u64()))]
    fn open_resolved(&mut self, terminal: AgentRef, resolved: Resolved) {
        let Resolved {
            launch,
            launched,
            attached,
            after_start,
        } = resolved;
        self.outbox.push(EventKind::TerminalLaunched(launched));
        let shared = self.launches.view(terminal.clone());
        self.terminals
            .open(terminal.clone(), launch, shared, &mut self.outbox);
        if let Some(prompt) = after_start {
            self.typist.prompt(terminal.clone(), prompt, Instant::now());
        }
        let Some((attached, line)) = attached else {
            return;
        };
        let started = self.jobs.run(move || {
            if let Err(error) = prepare_opencode_session(
                &attached.server,
                &attached.resume,
                &attached.cwd,
                attached.model.as_ref(),
            ) {
                tracing::debug!(%error, "could not prepare the opencode session");
            }
            let line = attach_line(&line, &attached.server.url(), &attached.resume);
            JobDone::AttachedLine(AttachedLine {
                terminal,
                bytes: typed(LaunchSite::Native, &line),
            })
        });
        if let Err(error) = started {
            tracing::warn!(%error, "could not type the opencode line");
        }
    }

    pub(super) fn attached_line_ready(&mut self, line: AttachedLine) {
        let write = TerminalCommand::Write {
            terminal: line.terminal,
            bytes: line.bytes,
            by: WriteBy::App,
        };
        drop(self.terminals.execute(write, &mut self.outbox));
    }

    fn resolve(
        &self,
        waiting: &WaitingLaunch,
        host: &Host,
        remote: Option<&krad_protocol::RemoteBusInfo>,
    ) -> Option<Resolved> {
        let environment = &waiting.terminal.environment;
        let data = self.environments.data(environment)?;
        let host_link = self.hosts.link(host);
        let wsl = waiting
            .cwd
            .as_deref()
            .filter(|_| host_link.is_local())
            .and_then(parse_wsl_path);
        let site = match (&host_link, &wsl) {
            (HostLink::Remote(_), _) => LaunchSite::Ssh,
            (HostLink::Local, Some(_)) => LaunchSite::Wsl,
            (HostLink::Local, None) => LaunchSite::Native,
        };
        let (program, args) = match &wsl {
            Some(folder) => wsl_shell_in(&folder.distro, &folder.linux),
            None => default_shell(),
        };
        let settings = &self.settings.current;
        let bus = self.bus.as_ref().map(crate::bus::Bus::access);
        let context = LineContext {
            settings,
            site,
            bus,
            remote,
            role: if data.session_led_by(waiting.terminal.terminal).is_some() {
                AgentRole::Lead
            } else {
                AgentRole::Member
            },
            tools_offered: settings.code_tools == CodeTools::On
                || settings.extension_tools == ExtensionTools::On
                || self.kraddies.offers_memory_tools(),
        };
        let agent = context.line(&waiting.intent);
        let harness = waiting.intent.harness();
        let attached = self.attach(waiting, harness, site);
        let caller = Caller::Terminal(TerminalHandle {
            environment: environment.as_str().to_owned(),
            terminal: waiting.terminal.terminal.as_u64(),
        });
        let mut vars = match (bus, host.is_local()) {
            (Some(bus), true) => local_caller_vars(bus, &caller, site.is_wsl()),
            (Some(_), false) => {
                remote.map_or_else(Vec::new, |remote| remote_caller_vars(remote, &caller))
            }
            (None, _) => Vec::new(),
        };
        if let Some(attached) = &attached {
            vars.extend(attached.server.client_env());
        }
        let launched_as = match (&agent, harness, site) {
            (None, None, LaunchSite::Wsl) => LaunchedAs::WslShell,
            (None, None, LaunchSite::Native | LaunchSite::Ssh) => LaunchedAs::Shell,
            _ => LaunchedAs::Line,
        };
        let (line, after_start) =
            agent.map_or((None, None), |(line, after): AgentLine| (Some(line), after));
        let launched = TerminalLaunched {
            terminal: waiting.terminal.clone(),
            harness,
            launched_as,
            resume: attached.as_ref().map(|attached| attached.resume.clone()),
        };
        let (typed_now, attached) = match (line, attached) {
            (Some(line), Some(attached)) => (None, Some((attached, line))),
            (Some(line), None) => (Some(typed(site, &line)), None),
            (None, _) => (None, None),
        };
        let launch = TerminalLaunch {
            program,
            args,
            cwd: waiting.cwd.clone(),
            vars,
            place: TerminalPlace::Host {
                link: host_link,
                shell: HostShell::Login,
                in_wsl: wsl.is_some(),
            },
            harness,
            prefill: waiting.prefill.clone(),
            typed: typed_now,
            tap: None,
        };
        Some(Resolved {
            launch,
            launched,
            attached,
            after_start,
        })
    }

    fn attach(
        &self,
        waiting: &WaitingLaunch,
        harness: Option<AgentPreset>,
        site: LaunchSite,
    ) -> Option<Attached> {
        if !harness.is_some_and(AgentPreset::attaches_to_server) || site != LaunchSite::Native {
            return None;
        }
        let server = self.opencode.server(&Host::Local)?;
        let resume = match &waiting.intent {
            LaunchIntent::Resume {
                resume: Some(resume),
                ..
            } => resume.clone(),
            LaunchIntent::Preset { .. }
            | LaunchIntent::Agent { .. }
            | LaunchIntent::Resume { .. }
            | LaunchIntent::Task { .. } => new_opencode_session_id(),
        };
        Some(Attached {
            server,
            resume,
            cwd: waiting.cwd.clone().unwrap_or_default(),
            model: waiting.intent.model().cloned(),
        })
    }
}
