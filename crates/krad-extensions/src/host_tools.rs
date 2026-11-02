use krad_domain::ServerRuntimeId;
use krad_host::{HostCommand, HostLink};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use ureq::Agent;

use crate::archive::kind_of;
use crate::error::ExtensionError;
use crate::manifest::{Manifests, ServerRuntimeManifest};
use crate::package::{fetch_to, output, Download};
use crate::platform::Platform;
use crate::runtime_requirements::RuntimeRequirements;
use crate::server_runtime::{
    download_for, download_name, home_vars, meets_minimum, source_digest, FindServerRuntime, BIN,
    SEARCH_DEPTH,
};

const FIND_SCRIPT: &str = r#"command -v "$1""#;
const PATH_VARIABLE: &str = "PATH";
const REMOTE_PATH_SEPARATOR: &str = ":";
const ROOT: char = '/';

pub(crate) fn find_on_host(link: &HostLink, name: &str) -> Option<PathBuf> {
    let mut command = HostCommand::posix_shell(FIND_SCRIPT);
    command.arg(name).in_login_shell();
    let output = link.output(&command).ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(ROOT))
        .map(PathBuf::from)
}

pub(crate) fn runs_on_host(link: &HostLink, program: &Path, args: &[String], cwd: &Path) -> bool {
    let mut command = HostCommand::new(program);
    command.args(args).current_dir(cwd).in_login_shell();
    link.output(&command)
        .is_ok_and(|output| output.status.success())
}

pub(crate) struct HostServerRuntimes<'a> {
    pub(crate) manifests: &'a Manifests,
    pub(crate) link: &'a HostLink,
    pub(crate) managed: Option<PathBuf>,
    pub(crate) requirements: &'a RuntimeRequirements,
}

impl HostServerRuntimes<'_> {
    fn usable(&self, server_runtime: &ServerRuntimeManifest, program: &Path) -> bool {
        let mut version = HostCommand::new(program);
        version.args(&server_runtime.version_args).in_login_shell();
        let Ok(output) = self.link.output(&version) else {
            return false;
        };
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        meets_minimum(
            server_runtime,
            self.requirements.get(&server_runtime.id),
            output.status.success(),
            &text,
        )
    }

    fn in_folder(
        &self,
        server_runtime: &ServerRuntimeManifest,
        folder: &Path,
        depth: usize,
    ) -> Option<PathBuf> {
        let found = [
            folder.join(&server_runtime.program),
            folder.join(BIN).join(&server_runtime.program),
        ]
        .into_iter()
        .find(|candidate| {
            self.link
                .metadata(candidate)
                .is_ok_and(|metadata| !metadata.is_dir())
                && self.usable(server_runtime, candidate)
        });
        if found.is_some() || depth == 0 {
            return found;
        }
        self.link
            .read_dir(folder)
            .ok()?
            .into_iter()
            .filter(|entry| entry.metadata.is_dir() && !entry.symlink)
            .find_map(|entry| self.in_folder(server_runtime, &entry.path, depth - 1))
    }

    fn managed_folder(&self, id: &ServerRuntimeId) -> Option<PathBuf> {
        self.managed.as_ref().map(|root| root.join(id.as_str()))
    }

    pub(crate) fn installable(&self, id: &ServerRuntimeId, platform: Platform) -> bool {
        self.managed.is_some()
            && self
                .manifests
                .server_runtime(id)
                .is_some_and(|server_runtime| download_for(server_runtime, platform).is_ok())
    }

    pub(crate) fn install(
        &self,
        id: &ServerRuntimeId,
        platform: Platform,
        agent: &Agent,
    ) -> Result<PathBuf, ExtensionError> {
        let server_runtime = self
            .manifests
            .server_runtime(id)
            .ok_or_else(|| ExtensionError::MissingServerRuntime(id.clone()))?;
        let folder = self
            .managed_folder(id)
            .ok_or(ExtensionError::Host(krad_host::HostError::NotConnected))?;
        let source = download_for(server_runtime, platform)?;
        let name = download_name(server_runtime, source);
        let digest = source_digest(agent, source, &name)?;
        if self.link.exists(&folder) {
            self.link.remove_dir_all(&folder)?;
        }
        let fetched = fetch_to(
            self.link,
            agent,
            platform.os,
            &Download {
                url: &source.url,
                name: &name,
                kind: kind_of(&name),
                digest: digest.as_ref(),
            },
            &folder,
        )
        .and_then(|()| self.run_setup(&folder, &name, source.run.as_deref()));
        if let Err(error) = fetched {
            if let Err(cleanup) = self.link.remove_dir_all(&folder) {
                tracing::warn!(error = %cleanup, "could not remove a failed server runtime install");
            }
            return Err(error);
        }
        self.find(id)
            .ok_or_else(|| ExtensionError::Missing(server_runtime.name.clone()))
    }

    fn run_setup(
        &self,
        folder: &Path,
        name: &str,
        args: Option<&[String]>,
    ) -> Result<(), ExtensionError> {
        let Some(args) = args else {
            return Ok(());
        };
        let mut setup = HostCommand::new(folder.join(name));
        setup.args(args).current_dir(folder);
        let ran = self.link.output(&setup)?;
        output::checked(name, &ran)
    }
}

impl FindServerRuntime for HostServerRuntimes<'_> {
    fn find(&self, id: &ServerRuntimeId) -> Option<PathBuf> {
        let server_runtime = self.manifests.server_runtime(id)?;
        find_on_host(self.link, &server_runtime.program)
            .filter(|program| self.usable(server_runtime, program))
            .or_else(|| {
                let folder = self.managed_folder(id)?;
                self.in_folder(server_runtime, &folder, SEARCH_DEPTH)
            })
    }

    fn launch_env(&self, found: &[(ServerRuntimeId, PathBuf)]) -> Vec<(String, OsString)> {
        let bins: Vec<String> = found
            .iter()
            .filter_map(|(_, program)| program.parent())
            .map(krad_host::remote_path)
            .collect();
        let mut vars = Vec::new();
        if !bins.is_empty() {
            vars.push((
                PATH_VARIABLE.to_owned(),
                OsString::from(bins.join(REMOTE_PATH_SEPARATOR)),
            ));
        }
        vars.extend(home_vars(self.manifests, found));
        vars
    }
}
