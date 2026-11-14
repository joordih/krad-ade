use krad_chat::Runner;
use krad_domain::{parse_wsl_path, AgentPreset, EnvironmentName, CHECKED_HARNESSES};
use krad_host::HostLink;
use krad_protocol::{
    AgentPlace, ErrorCode, HarnessSpot, HealthCommand, HealthPlace, ModelsCommand, OpencodeCommand,
    Reply, UsageNote,
};
use std::path::{Path, PathBuf};

use super::{refused, Runtime};
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::harness_health::HarnessProbe;
use crate::harness_sites::{launch_program, HarnessHere, NotHere, NO_PROGRAM_TEXT};
use crate::opencode::{OpencodeInput, Reach};
use crate::usage::{SourceState, UsagePlace, UsageSource};

const USAGE_HARNESSES: [AgentPreset; 3] = [
    AgentPreset::ClaudeCode,
    AgentPreset::Codex,
    AgentPreset::Opencode,
];
const OPENCODE_FAILED_TEXT: &str = "The opencode server did not start";

type Sited = Result<(Runner, PathBuf), NotHere>;

pub(super) struct Spotted {
    pub(super) link: HostLink,
    pub(super) root: Option<PathBuf>,
    environment: Option<EnvironmentName>,
}

impl Spotted {
    fn health_place(&self) -> HealthPlace {
        if !self.link.is_local() {
            return HealthPlace::Ssh {
                host: self.link.host(),
            };
        }
        match self.root.as_deref().and_then(parse_wsl_path) {
            Some(folder) => HealthPlace::Wsl {
                distro: folder.distro,
            },
            None => HealthPlace::ThisComputer,
        }
    }

    pub(super) fn home(&self) -> Option<PathBuf> {
        match &self.link {
            HostLink::Local => std::env::home_dir(),
            HostLink::Remote(ssh) => ssh.home().map(Path::to_path_buf),
        }
    }
}

fn runner_source(
    sited: Sited,
    broken: String,
    source: fn(Runner, PathBuf) -> UsageSource,
) -> SourceState {
    match sited {
        Ok((runner, cwd)) => SourceState::Ready(source(runner, cwd)),
        Err(NotHere::Waiting) => SourceState::Waiting(UsageNote::HostNotReady),
        Err(NotHere::NoProgram) => SourceState::Broken(broken),
    }
}

impl Runtime {
    pub(super) fn execute_health(&mut self, command: HealthCommand) -> Option<Reply> {
        let HealthCommand::Check { spot } = command else {
            return None;
        };
        let Some(spotted) = self.spotted(&spot) else {
            return Some(refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT));
        };
        let place = spotted.health_place();
        let probes = CHECKED_HARNESSES
            .into_iter()
            .map(|harness| (harness, self.health_probe(&place, &spotted, harness)))
            .collect();
        self.health
            .check(&place, probes, &self.jobs, &mut self.outbox);
        Some(Reply::Accepted)
    }

    pub(super) fn place_usage(&mut self, spot: Option<&HarnessSpot>) -> Reply {
        let place = match spot {
            None => None,
            Some(spot) => {
                let Some(spotted) = self.spotted(spot) else {
                    return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
                };
                Some(self.usage_place(&spotted))
            }
        };
        self.usage.place(place, &self.jobs, &mut self.outbox);
        Reply::Accepted
    }

    pub(super) fn list_models(&mut self, command: &ModelsCommand) -> Option<Reply> {
        let ModelsCommand::List { spot, harness } = command else {
            return None;
        };
        let Some(spotted) = self.spotted(spot) else {
            return Some(refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT));
        };
        if let AgentPreset::Acp(agent) = harness {
            self.list_acp_offers(&spotted, *agent);
            return Some(Reply::Accepted);
        }
        if let Ok((runner, cwd)) = self.spot_runner(&spotted, *harness) {
            self.models
                .list((spotted.link.host(), *harness), runner, cwd, &self.jobs);
        }
        Some(Reply::Accepted)
    }

    pub(super) fn execute_opencode(&mut self, command: OpencodeCommand) -> Option<Reply> {
        let input = match command {
            OpencodeCommand::Start { spot } => {
                let Some(spotted) = self.spotted(&spot) else {
                    return Some(refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT));
                };
                let Ok((runner, cwd)) = self.spot_runner(&spotted, AgentPreset::Opencode) else {
                    return Some(Reply::Accepted);
                };
                OpencodeInput::Start {
                    host: spotted.link.host(),
                    runner,
                    cwd,
                }
            }
            OpencodeCommand::Refresh { host } => OpencodeInput::Refresh(host),
            OpencodeCommand::Unknown => return None,
        };
        self.opencode.apply(input, &self.jobs, &mut self.outbox);
        Some(Reply::Accepted)
    }

    pub(super) fn forget_unshared_opencode(&mut self, environment: &EnvironmentName) {
        if self.hosts.shares_connection(environment) {
            return;
        }
        if let Some(host) = self.environments.host_of(environment) {
            self.opencode
                .apply(OpencodeInput::Forget(host), &self.jobs, &mut self.outbox);
        }
    }

    pub(super) fn spotted(&self, spot: &HarnessSpot) -> Option<Spotted> {
        let link = match &spot.environment {
            Some(environment) => self.hosts.link(&self.environments.host_of(environment)?),
            None => HostLink::Local,
        };
        Some(Spotted {
            link,
            root: spot.root.clone(),
            environment: spot.environment.clone(),
        })
    }

    pub(super) fn host_agents(&self, spotted: &Spotted) -> Option<&[AgentPreset]> {
        spotted
            .environment
            .as_ref()
            .and_then(|environment| self.hosts.agents(environment))
    }

    pub(super) fn spot_runner(&self, spotted: &Spotted, harness: AgentPreset) -> Sited {
        let program = launch_program(&self.settings.current, harness).ok_or(NotHere::NoProgram)?;
        let here = HarnessHere {
            link: &spotted.link,
            root: spotted.root.as_deref(),
            host_agents: self.host_agents(spotted),
        };
        here.runner(harness, program)
    }

    fn installed_at(&self, spotted: &Spotted, harness: AgentPreset) -> bool {
        let found = if spotted.link.is_local() {
            self.hosts.local_agents(&AgentPlace::Native)
        } else {
            self.host_agents(spotted)
        };
        found.is_some_and(|found| found.contains(&harness))
    }

    fn health_probe(
        &self,
        place: &HealthPlace,
        spotted: &Spotted,
        harness: AgentPreset,
    ) -> Option<HarnessProbe> {
        if let HealthPlace::Wsl { distro } = place {
            let wsl = AgentPlace::Wsl {
                distro: distro.clone(),
            };
            let found = self.hosts.local_agents(&wsl);
            if !found.is_some_and(|found| found.contains(&harness)) {
                return Some(HarnessProbe::Missing);
            }
        }
        match self.spot_runner(spotted, harness) {
            Ok((runner, cwd)) => Some(HarnessProbe::Run(runner, cwd)),
            Err(NotHere::Waiting) => None,
            Err(NotHere::NoProgram) => Some(HarnessProbe::OffPath {
                link: spotted.link.clone(),
                home: spotted.home(),
                program: launch_program(&self.settings.current, harness)?,
            }),
        }
    }

    fn usage_place(&mut self, spotted: &Spotted) -> UsagePlace {
        let host = spotted.link.host();
        let harnesses: Vec<AgentPreset> = USAGE_HARNESSES
            .into_iter()
            .filter(|harness| {
                self.installed_at(spotted, *harness)
                    || self.usage.has_reading(&(host.clone(), *harness))
            })
            .collect();
        let sources = harnesses
            .into_iter()
            .map(|harness| (harness, self.usage_source(spotted, harness)))
            .collect();
        UsagePlace { host, sources }
    }

    fn usage_source(&mut self, spotted: &Spotted, harness: AgentPreset) -> SourceState {
        let sited = self.spot_runner(spotted, harness);
        let broken = format!(
            "{} {NO_PROGRAM_TEXT}",
            self.settings.current.acp.harness_name(harness)
        );
        match harness {
            AgentPreset::Opencode => self.opencode_source(spotted, sited),
            AgentPreset::ClaudeCode => runner_source(sited, broken, UsageSource::Claude),
            AgentPreset::Codex => runner_source(sited, broken, UsageSource::Codex),
            AgentPreset::Gateway | AgentPreset::Shell | AgentPreset::Acp(_) => {
                SourceState::Waiting(UsageNote::HostNotReady)
            }
        }
    }

    fn opencode_source(&mut self, spotted: &Spotted, sited: Sited) -> SourceState {
        let Ok((runner, cwd)) = sited else {
            return SourceState::Waiting(UsageNote::Starting);
        };
        let host = spotted.link.host();
        match self
            .opencode
            .reach(&host, (&runner, &cwd), &self.jobs, &mut self.outbox)
        {
            Reach::Ready(server) => SourceState::Ready(UsageSource::Opencode(server)),
            Reach::Waiting => SourceState::Waiting(UsageNote::Starting),
            Reach::Failed(error) => SourceState::Broken(format!("{OPENCODE_FAILED_TEXT}: {error}")),
        }
    }
}
