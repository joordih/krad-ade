use krad_chat::{acp_package_state, install_acp_package, Runner};
use krad_domain::{
    acp_package, built_in_acp_agents, parse_wsl_path, ssh_config_aliases, AcpAgentId, AcpPackage,
    AcpPackageState, EnvironmentName, NpmPackage,
};
use krad_host::{read_ssh_config, HostLink};
use krad_protocol::{
    AcpPackageShown, Answer, ErrorCode, HarnessSpot, HostCommand, Reply, RepoCommand, RequestId,
    SubagentsAsk, TranscriptAsk,
};
use krad_session::find_program;
use std::path::{Path, PathBuf};

use super::harness_spots::Spotted;
use super::{refused, Runtime, UNSUPPORTED_COMMAND};
use crate::chats::{read_subagents, read_tail};
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::jobs::JobDone;
use crate::repo_asks;

const NPM_PROGRAM: &str = "npm";
const NO_NPM_TEXT: &str = "npm was not found; install Node.js to install this agent";
const NO_HOME_TEXT: &str = "the host's home folder is not known yet";

type NpmSite = Result<(Runner, PathBuf), String>;

fn npm_site(spotted: &Spotted) -> NpmSite {
    if let HostLink::Remote(ssh) = &spotted.link {
        let home = ssh.home().map(PathBuf::from).ok_or(NO_HOME_TEXT)?;
        let runner = Runner::Ssh {
            link: spotted.link.clone(),
            program: NPM_PROGRAM.to_owned(),
        };
        return Ok((runner, home));
    }
    if let Some((root, folder)) = spotted
        .root
        .as_ref()
        .and_then(|root| parse_wsl_path(root).map(|folder| (root, folder)))
    {
        let runner = Runner::Wsl {
            distro: folder.distro,
            program: NPM_PROGRAM.to_owned(),
        };
        return Ok((runner, root.clone()));
    }
    let npm = find_program(NPM_PROGRAM).ok_or(NO_NPM_TEXT)?;
    Ok((Runner::Native(npm), spotted.home().unwrap_or_default()))
}

fn npm_packages() -> Vec<(AcpAgentId, NpmPackage)> {
    built_in_acp_agents()
        .into_iter()
        .filter_map(|agent| Some((agent.id, acp_package(agent.id)?.npm()?)))
        .collect()
}

fn package_state(
    site: &NpmSite,
    package: NpmPackage,
    act: fn(&Runner, &Path, NpmPackage) -> Result<AcpPackageState, String>,
) -> AcpPackageState {
    match site {
        Ok((npm, cwd)) => act(npm, cwd, package).unwrap_or_else(AcpPackageState::Failed),
        Err(reason) => AcpPackageState::Failed(reason.clone()),
    }
}

fn checked(npm: &Runner, cwd: &Path, package: NpmPackage) -> Result<AcpPackageState, String> {
    acp_package_state(npm, cwd, package).map_err(|error| error.to_string())
}

fn installed(npm: &Runner, cwd: &Path, package: NpmPackage) -> Result<AcpPackageState, String> {
    install_acp_package(npm, cwd, package).map_err(|error| error.to_string())
}

impl Runtime {
    pub(super) fn answer_in_job(
        &self,
        request: RequestId,
        work: impl FnOnce() -> Answer + Send + 'static,
    ) -> Reply {
        let started = self.jobs.run(move || JobDone::Answered {
            request,
            answer: Box::new(work()),
        });
        match started {
            Ok(()) => Reply::Accepted,
            Err(error) => refused(ErrorCode::Unavailable, &error.to_string()),
        }
    }

    pub(super) fn execute_repo(&self, id: RequestId, command: RepoCommand) -> Reply {
        let Some(host) = self.environments.host_of(&command.environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let link = self.hosts.link(&host);
        self.answer_in_job(id, move || repo_asks::answer(&link, command.ask))
    }

    pub(super) fn ask_transcript(
        &self,
        id: RequestId,
        environment: &EnvironmentName,
        ask: TranscriptAsk,
    ) -> Reply {
        let Some(host) = self.environments.host_of(environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let link = self.hosts.link(&host);
        self.answer_in_job(id, move || {
            Answer::Transcript(Box::new(read_tail(&link, ask)))
        })
    }

    pub(super) fn ask_subagents(
        &self,
        id: RequestId,
        environment: &EnvironmentName,
        ask: SubagentsAsk,
    ) -> Reply {
        let Some(host) = self.environments.host_of(environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let link = self.hosts.link(&host);
        self.answer_in_job(id, move || {
            Answer::Subagents(Box::new(read_subagents(&link, ask)))
        })
    }

    pub(super) fn execute_host_ask(&self, id: RequestId, command: HostCommand) -> Reply {
        match command {
            HostCommand::SshHosts => self.answer_in_job(id, || Answer::SshHosts {
                aliases: std::env::home_dir().map_or_else(Vec::new, |home| {
                    ssh_config_aliases(&read_ssh_config(&HostLink::Local, &home))
                }),
            }),
            HostCommand::AcpPackages { spot } => self.ask_packages(id, &spot, None),
            HostCommand::InstallAcpPackage { spot, agent } => {
                self.ask_packages(id, &spot, Some(agent))
            }
            _ => refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND),
        }
    }

    fn ask_packages(
        &self,
        id: RequestId,
        spot: &HarnessSpot,
        install: Option<AcpAgentId>,
    ) -> Reply {
        let Some(spotted) = self.spotted(spot) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let site = npm_site(&spotted);
        let wanted: Vec<(AcpAgentId, NpmPackage)> = match install {
            Some(agent) => acp_package(agent)
                .and_then(AcpPackage::npm)
                .map(|package| (agent, package))
                .into_iter()
                .collect(),
            None => npm_packages(),
        };
        let act = if install.is_some() {
            installed
        } else {
            checked
        };
        self.answer_in_job(id, move || Answer::AcpPackages {
            packages: wanted
                .into_iter()
                .map(|(agent, package)| AcpPackageShown {
                    agent,
                    state: package_state(&site, package, act),
                })
                .collect(),
        })
    }
}
