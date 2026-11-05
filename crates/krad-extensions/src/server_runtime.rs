use krad_domain::{ServerRuntimeId, ServerRuntimePaths, ToolchainVersion, VersionConstraint};
use krad_host::hide_console_window;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use ureq::Agent;

use crate::archive::place;
use crate::error::ExtensionError;
use crate::http::{download, fetch_text, Sha256Digest};
use crate::manifest::{Manifests, RuntimeDownload, RuntimeMinimum, ServerRuntimeManifest};
use crate::path_lookup::find_on_path;
use crate::platform::{Os, Platform};
use crate::runtime_requirements::RuntimeRequirements;

pub(crate) const SERVER_RUNTIMES_DIR: &str = "toolchains";
pub(crate) const BIN: &str = "bin";
const MAC_HOME: [&str; 2] = ["Contents", "Home"];
const HOME_VARIABLES: [&str; 2] = ["HOME", "USERPROFILE"];
const TILDE: &str = "~/";
const PATH_VARIABLE: &str = "PATH";
const URL_SEPARATOR: char = '/';
pub(crate) const SEARCH_DEPTH: usize = 5;

pub(crate) trait FindServerRuntime {
    fn find(&self, id: &ServerRuntimeId) -> Option<PathBuf>;
    fn launch_env(&self, found: &[(ServerRuntimeId, PathBuf)]) -> Vec<(String, OsString)>;
}

impl FindServerRuntime for ServerRuntimes<'_> {
    fn find(&self, id: &ServerRuntimeId) -> Option<PathBuf> {
        ServerRuntimes::find(self, id)
    }

    fn launch_env(&self, found: &[(ServerRuntimeId, PathBuf)]) -> Vec<(String, OsString)> {
        ServerRuntimes::launch_env(self, found)
    }
}

pub(crate) fn wanted_version(
    server_runtime: &ServerRuntimeManifest,
    required: Option<&VersionConstraint>,
) -> Option<VersionConstraint> {
    let minimum = server_runtime
        .minimum_version
        .as_ref()
        .map(RuntimeMinimum::constraint);
    match (minimum, required) {
        (Some(minimum), Some(required)) => Some(minimum.and(required)),
        (Some(minimum), None) => Some(minimum),
        (None, Some(required)) => Some(required.clone()),
        (None, None) => None,
    }
}

pub(crate) fn meets_minimum(
    server_runtime: &ServerRuntimeManifest,
    required: Option<&VersionConstraint>,
    success: bool,
    text: &str,
) -> bool {
    match wanted_version(server_runtime, required) {
        None => success,
        Some(wanted) => version_of(text, server_runtime.version_after.as_deref())
            .is_some_and(|found| wanted.matches(&found)),
    }
}

pub(crate) fn home_vars(
    manifests: &Manifests,
    found: &[(ServerRuntimeId, PathBuf)],
) -> Vec<(String, OsString)> {
    let mut vars = Vec::new();
    for (id, program) in found {
        let named = manifests
            .server_runtime(id)
            .and_then(|server_runtime| server_runtime.launch_home_var.clone());
        let home = program.parent().and_then(Path::parent);
        if let (Some(name), Some(home)) = (named, home) {
            vars.push((name, home.as_os_str().to_owned()));
        }
    }
    vars
}

fn version_of(output: &str, after: Option<&str>) -> Option<ToolchainVersion> {
    let text = match after {
        Some(marker) => &output[output.find(marker)? + marker.len()..],
        None => output,
    };
    let start = text.find(|character: char| character.is_ascii_digit())?;
    text[start..].split_whitespace().next()?.parse().ok()
}

fn home() -> Option<PathBuf> {
    HOME_VARIABLES
        .iter()
        .find_map(|name| env::var_os(name).filter(|value| !value.is_empty()))
        .map(PathBuf::from)
}

fn expand(folder: &str) -> Option<PathBuf> {
    match folder.strip_prefix(TILDE) {
        Some(rest) => home().map(|home| home.join(rest)),
        None => Some(PathBuf::from(folder)),
    }
}

pub(crate) struct ServerRuntimes<'a> {
    pub(crate) manifests: &'a Manifests,
    pub(crate) root: &'a Path,
    pub(crate) platform: Platform,
    pub(crate) chosen: &'a ServerRuntimePaths,
    pub(crate) requirements: &'a RuntimeRequirements,
}

impl ServerRuntimes<'_> {
    fn program(&self, server_runtime: &ServerRuntimeManifest) -> String {
        let base = match self.platform.os {
            Os::Windows => server_runtime
                .windows_program
                .clone()
                .unwrap_or_else(|| server_runtime.program.clone()),
            Os::Macos | Os::Linux => server_runtime.program.clone(),
        };
        format!("{base}{}", self.platform.exe())
    }

    fn usable(&self, server_runtime: &ServerRuntimeManifest, program: &Path) -> bool {
        let mut command = Command::new(program);
        command
            .args(&server_runtime.version_args)
            .stdin(Stdio::null());
        hide_console_window(&mut command);
        let Ok(output) = command.output() else {
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

    fn in_folder_of(
        &self,
        server_runtime: &ServerRuntimeManifest,
        folder: &Path,
    ) -> Option<PathBuf> {
        let name = self.program(server_runtime);
        let mac_home = MAC_HOME
            .iter()
            .fold(folder.to_path_buf(), |path, part| path.join(part));
        [
            folder.join(&name),
            folder.join(BIN).join(&name),
            mac_home.join(BIN).join(&name),
        ]
        .into_iter()
        .find(|candidate| candidate.is_file() && self.usable(server_runtime, candidate))
    }

    fn walk(
        &self,
        server_runtime: &ServerRuntimeManifest,
        folder: &Path,
        depth: usize,
    ) -> Option<PathBuf> {
        if let Some(found) = self.in_folder_of(server_runtime, folder) {
            return Some(found);
        }
        if depth == 0 {
            return None;
        }
        fs::read_dir(folder)
            .ok()?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .find_map(|entry| self.walk(server_runtime, &entry.path(), depth - 1))
    }

    fn system(&self, server_runtime: &ServerRuntimeManifest) -> Option<PathBuf> {
        let from_vars = server_runtime
            .home_vars
            .iter()
            .filter_map(|name| env::var_os(name).filter(|value| !value.is_empty()))
            .find_map(|home| self.in_folder_of(server_runtime, Path::new(&home)));
        from_vars
            .or_else(|| {
                find_on_path(&server_runtime.program)
                    .filter(|found| self.usable(server_runtime, found))
            })
            .or_else(|| {
                server_runtime
                    .search
                    .iter()
                    .filter_map(|folder| expand(folder))
                    .find_map(|folder| self.in_folder_of(server_runtime, &folder))
            })
    }

    fn managed_folder(&self, id: &ServerRuntimeId) -> PathBuf {
        self.root.join(SERVER_RUNTIMES_DIR).join(id.as_str())
    }

    pub(crate) fn find(&self, id: &ServerRuntimeId) -> Option<PathBuf> {
        let server_runtime = self.manifests.server_runtime(id)?;
        self.chosen
            .get(id)
            .and_then(|folder| self.in_folder_of(server_runtime, folder))
            .or_else(|| self.system(server_runtime))
            .or_else(|| self.walk(server_runtime, &self.managed_folder(id), SEARCH_DEPTH))
    }

    pub(crate) fn accepts(&self, id: &ServerRuntimeId, folder: &Path) -> bool {
        self.manifests
            .server_runtime(id)
            .and_then(|server_runtime| self.in_folder_of(server_runtime, folder))
            .is_some()
    }

    pub(crate) fn installable(&self, id: &ServerRuntimeId) -> bool {
        self.manifests
            .server_runtime(id)
            .is_some_and(|server_runtime| {
                server_runtime
                    .download
                    .iter()
                    .any(|download| self.platform.matches(&download.targets))
            })
    }

    pub(crate) fn install(
        &self,
        id: &ServerRuntimeId,
        agent: &Agent,
    ) -> Result<PathBuf, ExtensionError> {
        let server_runtime = self
            .manifests
            .server_runtime(id)
            .ok_or_else(|| ExtensionError::MissingServerRuntime(id.clone()))?;
        let source = download_for(server_runtime, self.platform)?;
        let name = download_name(server_runtime, source);
        let digest = source_digest(agent, source, &name)?;
        let folder = self.managed_folder(id);
        if folder.exists() {
            fs::remove_dir_all(&folder)?;
        }
        fs::create_dir_all(&folder)?;
        let downloaded = folder.join(format!("{name}.download"));
        download(agent, &source.url, digest.as_ref(), &downloaded)?;
        place(&downloaded, &name, &folder, self.platform.os)?;
        fs::remove_file(&downloaded)?;
        if let Some(args) = &source.run {
            let mut command = Command::new(folder.join(&name));
            command
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            hide_console_window(&mut command);
            let status = command.status()?;
            if !status.success() {
                return Err(ExtensionError::Tool(name));
            }
        }
        self.find(id)
            .ok_or_else(|| ExtensionError::Missing(server_runtime.name.clone()))
    }

    pub(crate) fn launch_env(
        &self,
        found: &[(ServerRuntimeId, PathBuf)],
    ) -> Vec<(String, OsString)> {
        let bins = found
            .iter()
            .filter_map(|(_, program)| program.parent().map(Path::to_path_buf));
        let inherited = env::var_os(PATH_VARIABLE).unwrap_or_default();
        let mut vars = Vec::new();
        if let Ok(path) = env::join_paths(bins.chain(env::split_paths(&inherited))) {
            vars.push((PATH_VARIABLE.to_owned(), path));
        }
        vars.extend(home_vars(self.manifests, found));
        vars
    }
}

pub(crate) fn download_for(
    server_runtime: &ServerRuntimeManifest,
    platform: Platform,
) -> Result<&RuntimeDownload, ExtensionError> {
    server_runtime
        .download
        .iter()
        .find(|download| platform.matches(&download.targets))
        .ok_or(ExtensionError::NoBuild)
}

pub(crate) fn download_name(
    server_runtime: &ServerRuntimeManifest,
    source: &RuntimeDownload,
) -> String {
    source
        .url
        .rsplit(URL_SEPARATOR)
        .next()
        .unwrap_or(server_runtime.program.as_str())
        .to_owned()
}

pub(crate) fn source_digest(
    agent: &Agent,
    source: &RuntimeDownload,
    name: &str,
) -> Result<Option<Sha256Digest>, ExtensionError> {
    Ok(match (&source.sha256_url, &source.sha256_list) {
        (Some(url), _) => {
            let text = fetch_text(agent, url)?;
            Some(Sha256Digest::new(
                text.split_whitespace().next().unwrap_or_default(),
            ))
        }
        (None, Some(list)) => fetch_text(agent, list)?
            .lines()
            .find(|line| line.split_whitespace().nth(1) == Some(name))
            .and_then(|line| line.split_whitespace().next())
            .map(Sha256Digest::new),
        (None, None) => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_read_after_their_marker() {
        let parts = |output: &str, after: Option<&str>| {
            version_of(output, after).map(|version| version.parts().to_vec())
        };
        assert_eq!(
            parts("openjdk version \"25.0.1\" 2025", Some("version \"")),
            Some(vec![25, 0, 1])
        );
        assert_eq!(parts("v24.22.1", None), Some(vec![24, 22, 1]));
        assert_eq!(parts("Python 3.9.6", None), Some(vec![3, 9, 6]));
        assert_eq!(parts("no digits", None), None);
        assert_eq!(
            parts("1.8 then version \"21\"", Some("version \"")),
            Some(vec![21])
        );
    }
}
