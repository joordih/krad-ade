use krad_chat::Runner;
use krad_domain::{parse_wsl_path, AgentPreset, Settings};
use krad_host::HostLink;
use krad_session::find_program;
use std::path::{Path, PathBuf};

pub(crate) const NO_PROGRAM_TEXT: &str = "is not on PATH";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotHere {
    Waiting,
    NoProgram,
}

pub(crate) struct HarnessHere<'a> {
    pub(crate) link: &'a HostLink,
    pub(crate) root: Option<&'a Path>,
    pub(crate) host_agents: Option<&'a [AgentPreset]>,
}

pub(crate) fn launch_program(settings: &Settings, harness: AgentPreset) -> Option<String> {
    settings
        .launch_line(harness)
        .and_then(|line| line.split_whitespace().next().map(str::to_owned))
}

impl HarnessHere<'_> {
    pub(crate) fn runner(
        &self,
        harness: AgentPreset,
        program: String,
    ) -> Result<(Runner, PathBuf), NotHere> {
        if let HostLink::Remote(ssh) = self.link {
            let found = self.host_agents.ok_or(NotHere::Waiting)?;
            if !found.contains(&harness) {
                return Err(NotHere::NoProgram);
            }
            let cwd = self
                .root
                .map(Path::to_path_buf)
                .or_else(|| ssh.home().map(Path::to_path_buf))
                .ok_or(NotHere::Waiting)?;
            let link = self.link.clone();
            return Ok((Runner::Ssh { link, program }, cwd));
        }
        let wsl = self
            .root
            .and_then(|root| parse_wsl_path(root).map(|inside| (root, inside)));
        if let Some((root, inside)) = wsl {
            let runner = Runner::Wsl {
                distro: inside.distro,
                program,
            };
            return Ok((runner, root.to_path_buf()));
        }
        let runner = find_program(&program)
            .map(Runner::Native)
            .ok_or(NotHere::NoProgram)?;
        let cwd = self
            .root
            .map(Path::to_path_buf)
            .or_else(std::env::home_dir)
            .unwrap_or_default();
        Ok((runner, cwd))
    }
}
