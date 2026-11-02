use krad_domain::{ToolchainId, ToolchainPaths};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use ureq::Agent;

use crate::archive::place;
use crate::error::ExtensionError;
use crate::http::{download, fetch_text, Sha256Digest};
use crate::manifest::{Manifests, ToolchainManifest};
use crate::path_lookup::find_on_path;
use crate::platform::{Os, Platform};

const TOOLCHAINS_DIR: &str = "toolchains";
const BIN: &str = "bin";
const MAC_HOME: [&str; 2] = ["Contents", "Home"];
const HOME_VARIABLES: [&str; 2] = ["HOME", "USERPROFILE"];
const TILDE: &str = "~/";
const PATH_VARIABLE: &str = "PATH";
const URL_SEPARATOR: char = '/';
const SEARCH_DEPTH: usize = 5;

fn first_number(text: &str) -> Option<u32> {
    let start = text.find(|character: char| character.is_ascii_digit())?;
    let digits: String = text[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn version_of(output: &str, after: Option<&str>) -> Option<u32> {
    let text = match after {
        Some(marker) => &output[output.find(marker)? + marker.len()..],
        None => output,
    };
    first_number(text)
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

pub(crate) struct Toolchains<'a> {
    pub(crate) manifests: &'a Manifests,
    pub(crate) root: &'a Path,
    pub(crate) platform: Platform,
    pub(crate) chosen: &'a ToolchainPaths,
}

impl Toolchains<'_> {
    fn program(&self, toolchain: &ToolchainManifest) -> String {
        let base = match self.platform.os {
            Os::Windows => toolchain
                .windows_program
                .clone()
                .unwrap_or_else(|| toolchain.program.clone()),
            Os::Macos | Os::Linux => toolchain.program.clone(),
        };
        format!("{base}{}", self.platform.exe())
    }

    fn usable(toolchain: &ToolchainManifest, program: &Path) -> bool {
        let Ok(output) = Command::new(program)
            .args(&toolchain.version_args)
            .stdin(Stdio::null())
            .output()
        else {
            return false;
        };
        let Some(minimum) = toolchain.minimum_version else {
            return output.status.success();
        };
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        version_of(&text, toolchain.version_after.as_deref()).is_some_and(|found| found >= minimum)
    }

    fn in_folder_of(&self, toolchain: &ToolchainManifest, folder: &Path) -> Option<PathBuf> {
        let name = self.program(toolchain);
        let mac_home = MAC_HOME
            .iter()
            .fold(folder.to_path_buf(), |path, part| path.join(part));
        [
            folder.join(&name),
            folder.join(BIN).join(&name),
            mac_home.join(BIN).join(&name),
        ]
        .into_iter()
        .find(|candidate| candidate.is_file() && Self::usable(toolchain, candidate))
    }

    fn walk(&self, toolchain: &ToolchainManifest, folder: &Path, depth: usize) -> Option<PathBuf> {
        if let Some(found) = self.in_folder_of(toolchain, folder) {
            return Some(found);
        }
        if depth == 0 {
            return None;
        }
        fs::read_dir(folder)
            .ok()?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .find_map(|entry| self.walk(toolchain, &entry.path(), depth - 1))
    }

    fn system(&self, toolchain: &ToolchainManifest) -> Option<PathBuf> {
        let from_vars = toolchain
            .home_vars
            .iter()
            .filter_map(|name| env::var_os(name).filter(|value| !value.is_empty()))
            .find_map(|home| self.in_folder_of(toolchain, Path::new(&home)));
        from_vars
            .or_else(|| {
                find_on_path(&toolchain.program).filter(|found| Self::usable(toolchain, found))
            })
            .or_else(|| {
                toolchain
                    .search
                    .iter()
                    .filter_map(|folder| expand(folder))
                    .find_map(|folder| self.in_folder_of(toolchain, &folder))
            })
    }

    fn managed_folder(&self, id: &ToolchainId) -> PathBuf {
        self.root.join(TOOLCHAINS_DIR).join(id.as_str())
    }

    pub(crate) fn find(&self, id: &ToolchainId) -> Option<PathBuf> {
        let toolchain = self.manifests.toolchain(id)?;
        self.chosen
            .get(id)
            .and_then(|folder| self.in_folder_of(toolchain, folder))
            .or_else(|| self.system(toolchain))
            .or_else(|| self.walk(toolchain, &self.managed_folder(id), SEARCH_DEPTH))
    }

    pub(crate) fn accepts(&self, id: &ToolchainId, folder: &Path) -> bool {
        self.manifests
            .toolchain(id)
            .and_then(|toolchain| self.in_folder_of(toolchain, folder))
            .is_some()
    }

    pub(crate) fn installable(&self, id: &ToolchainId) -> bool {
        self.manifests.toolchain(id).is_some_and(|toolchain| {
            toolchain
                .download
                .iter()
                .any(|download| self.platform.matches(&download.targets))
        })
    }

    pub(crate) fn install(
        &self,
        id: &ToolchainId,
        agent: &Agent,
    ) -> Result<PathBuf, ExtensionError> {
        let toolchain = self
            .manifests
            .toolchain(id)
            .ok_or_else(|| ExtensionError::MissingToolchain(id.clone()))?;
        let source = toolchain
            .download
            .iter()
            .find(|download| self.platform.matches(&download.targets))
            .ok_or(ExtensionError::NoBuild)?;
        let name = source
            .url
            .rsplit(URL_SEPARATOR)
            .next()
            .unwrap_or(toolchain.program.as_str())
            .to_owned();
        let digest = match (&source.sha256_url, &source.sha256_list) {
            (Some(url), _) => {
                let text = fetch_text(agent, url)?;
                Some(Sha256Digest::new(
                    text.split_whitespace().next().unwrap_or_default(),
                ))
            }
            (None, Some(list)) => fetch_text(agent, list)?
                .lines()
                .find(|line| line.split_whitespace().nth(1) == Some(name.as_str()))
                .and_then(|line| line.split_whitespace().next())
                .map(Sha256Digest::new),
            (None, None) => None,
        };
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
            let status = Command::new(folder.join(&name))
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            if !status.success() {
                return Err(ExtensionError::Tool(name));
            }
        }
        self.find(id)
            .ok_or_else(|| ExtensionError::Missing(toolchain.name.clone()))
    }

    pub(crate) fn launch_env(&self, found: &[(ToolchainId, PathBuf)]) -> Vec<(String, OsString)> {
        let bins = found
            .iter()
            .filter_map(|(_, program)| program.parent().map(Path::to_path_buf));
        let inherited = env::var_os(PATH_VARIABLE).unwrap_or_default();
        let mut vars = Vec::new();
        if let Ok(path) = env::join_paths(bins.chain(env::split_paths(&inherited))) {
            vars.push((PATH_VARIABLE.to_owned(), path));
        }
        for (id, program) in found {
            let named = self
                .manifests
                .toolchain(id)
                .and_then(|toolchain| toolchain.launch_home_var.clone());
            let home = program.parent().and_then(Path::parent);
            if let (Some(name), Some(home)) = (named, home) {
                vars.push((name, home.as_os_str().to_owned()));
            }
        }
        vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_read_after_their_marker() {
        assert_eq!(
            version_of("openjdk version \"25.0.1\" 2025", Some("version \"")),
            Some(25)
        );
        assert_eq!(version_of("v24.22.1", None), Some(24));
        assert_eq!(version_of("no digits", None), None);
        assert_eq!(
            version_of("1.8 then version \"21\"", Some("version \"")),
            Some(21)
        );
    }
}
