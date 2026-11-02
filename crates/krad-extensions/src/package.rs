use krad_domain::ToolchainId;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use ureq::Agent;

use crate::archive::place;
use crate::error::ExtensionError;
use crate::http::{download, release_by_tag};
use crate::manifest::{Manifests, PackageManager, Runner, ToolchainManifest};
use crate::platform::{Os, Platform};
use crate::registry::{Entry, Package};
use crate::template::{render, render_all, Vars};
use crate::toolchain::Toolchains;

const INSTALLED_MARK: &str = ".krad-installed";
const GITHUB: &str = "github";
const GENERIC: &str = "generic";
const EXEC: &str = "exec";
const FILE_FIELD: &str = "file";
const FILES_PREFIX: &str = "files.";
const DESTINATION: char = ':';
const TEMPLATE_START: &str = "{{";
const EXTRA_QUALIFIER: &str = "extra";
const REPOSITORY_QUALIFIER: &str = "repository_url";
const GITHUB_DOWNLOADS: &str = "https://github.com";
const PATH_VARIABLE: &str = "PATH";

pub(crate) enum Tool<'a> {
    Manager(&'a ToolchainManifest, &'a PackageManager),
    Runner(&'a ToolchainManifest, &'a Runner),
}

pub(crate) fn tool<'a>(manifests: &'a Manifests, kind: &str) -> Option<Tool<'a>> {
    manifests.toolchains.iter().find_map(|toolchain| {
        toolchain
            .manager
            .iter()
            .find(|manager| manager.kind == kind)
            .map(|manager| Tool::Manager(toolchain, manager))
            .or_else(|| {
                toolchain
                    .runner
                    .iter()
                    .find(|runner| runner.kind == kind)
                    .map(|runner| Tool::Runner(toolchain, runner))
            })
    })
}

fn bin_kind(value: &str) -> Option<(&str, &str)> {
    if value.starts_with(TEMPLATE_START) {
        return None;
    }
    value.split_once(DESTINATION)
}

pub(crate) fn needed_toolchains(
    manifests: &Manifests,
    package: &Package,
    bin: &str,
) -> Vec<ToolchainId> {
    let mut kinds = vec![package.purl.kind.as_str()];
    if let Some((kind, _)) = package.bins.get(bin).and_then(|value| bin_kind(value)) {
        kinds.push(kind);
    }
    let mut needed: Vec<ToolchainId> = Vec::new();
    for kind in kinds {
        let owner = match tool(manifests, kind) {
            Some(Tool::Manager(toolchain, _) | Tool::Runner(toolchain, _)) => &toolchain.id,
            None => continue,
        };
        if !needed.contains(owner) {
            needed.push(owner.clone());
        }
    }
    needed
}

pub(crate) struct Installing<'a> {
    pub(crate) manifests: &'a Manifests,
    pub(crate) toolchains: &'a Toolchains<'a>,
    pub(crate) platform: Platform,
    pub(crate) agent: &'a Agent,
}

impl Installing<'_> {
    pub(crate) fn vars(&self, package: &Package, dir: &Path) -> Result<Vars, ExtensionError> {
        let purl = &package.purl;
        let extra = purl
            .qualifiers
            .get(EXTRA_QUALIFIER)
            .map_or_else(String::new, |extra| format!("[{extra}]"));
        let mut vars = Vars::default()
            .with("version", purl.version.clone())
            .with("package", purl.name.clone())
            .with("dir", dir.to_string_lossy())
            .with("exe", self.platform.exe())
            .with("cmd", self.platform.cmd())
            .with("extra", extra);
        if let Some(repository) = purl.qualifiers.get(REPOSITORY_QUALIFIER) {
            vars.set(REPOSITORY_QUALIFIER, repository.clone());
        }
        if let Some(entry) = package.entry_for(self.platform) {
            let prefix = Package::entry_prefix(entry);
            for (key, value) in &entry.fields {
                let filled = render(value, &vars, dir)?;
                vars.set(format!("{prefix}.{key}"), filled);
            }
        }
        Ok(vars)
    }

    fn with_toolchain(
        &self,
        vars: Vars,
        toolchain: &ToolchainManifest,
    ) -> Result<Vars, ExtensionError> {
        let program = self
            .toolchains
            .find(&toolchain.id)
            .ok_or_else(|| ExtensionError::MissingToolchain(toolchain.id.clone()))?;
        let folder = program
            .parent()
            .map_or_else(String::new, |folder| folder.to_string_lossy().into_owned());
        Ok(vars
            .with("toolchain", program.to_string_lossy())
            .with("toolchain_dir", folder))
    }

    fn run(
        commands: &[Vec<String>],
        vars: &Vars,
        env: &[(String, String)],
        dir: &Path,
    ) -> Result<(), ExtensionError> {
        let toolchain_dir = vars.get("toolchain_dir").unwrap_or_default().to_owned();
        let inherited = std::env::var_os(PATH_VARIABLE).unwrap_or_default();
        let path = std::env::join_paths(
            std::iter::once(PathBuf::from(&toolchain_dir)).chain(std::env::split_paths(&inherited)),
        )
        .unwrap_or(inherited);
        for command in commands {
            let parts = render_all(command, vars, dir)?;
            let Some((program, args)) = parts.split_first() else {
                continue;
            };
            let status = Command::new(program)
                .args(args)
                .current_dir(dir)
                .env(PATH_VARIABLE, &path)
                .envs(env.iter().map(|(name, value)| (name, value)))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            if !status.success() {
                return Err(ExtensionError::Tool(program.clone()));
            }
        }
        Ok(())
    }

    fn fetch_github(
        &self,
        package: &Package,
        entry: &Entry,
        vars: &Vars,
        dir: &Path,
    ) -> Result<(), ExtensionError> {
        let file = vars
            .get(&format!("{}.{FILE_FIELD}", Package::entry_prefix(entry)))
            .ok_or(ExtensionError::NoBuild)?;
        let (name, destination) = file.split_once(DESTINATION).unwrap_or((file, ""));
        let repository = &package.purl.name;
        let version = &package.purl.version;
        let digest = release_by_tag(self.agent, repository, version)
            .ok()
            .and_then(|release| {
                release
                    .assets
                    .into_iter()
                    .find(|asset| asset.name == name)
                    .and_then(|asset| asset.sha256())
            });
        let url = format!("{GITHUB_DOWNLOADS}/{repository}/releases/download/{version}/{name}");
        let downloaded = dir.join(format!("{name}.download"));
        download(self.agent, &url, digest.as_ref(), &downloaded)?;
        place(&downloaded, name, &dir.join(destination), self.platform.os)?;
        fs::remove_file(&downloaded)?;
        Ok(())
    }

    fn fetch_generic(&self, entry: &Entry, vars: &Vars, dir: &Path) -> Result<(), ExtensionError> {
        let prefix = format!("{}.{FILES_PREFIX}", Package::entry_prefix(entry));
        let files: Vec<(String, String)> = entry
            .fields
            .keys()
            .filter_map(|key| key.strip_prefix(FILES_PREFIX))
            .filter_map(|name| {
                let url = vars.get(&format!("{prefix}{name}"))?;
                Some((name.to_owned(), url.to_owned()))
            })
            .collect();
        if files.is_empty() {
            return Err(ExtensionError::NoBuild);
        }
        for (name, url) in files {
            let downloaded = dir.join(format!("{name}.download"));
            download(self.agent, &url, None, &downloaded)?;
            place(&downloaded, &name, dir, self.platform.os)?;
            fs::remove_file(&downloaded)?;
        }
        Ok(())
    }

    fn manage(&self, package: &Package, vars: Vars, dir: &Path) -> Result<(), ExtensionError> {
        let kind = &package.purl.kind;
        let Some(Tool::Manager(toolchain, manager)) = tool(self.manifests, kind) else {
            return Err(ExtensionError::UnknownKind(kind.clone()));
        };
        let vars = self.with_toolchain(vars, toolchain)?;
        let commands = if vars.get(REPOSITORY_QUALIFIER).is_some() {
            manager
                .install_from_repository
                .as_ref()
                .unwrap_or(&manager.install)
        } else if self.platform.os == Os::Windows {
            manager.windows_install.as_ref().unwrap_or(&manager.install)
        } else {
            &manager.install
        };
        let env: Vec<(String, String)> = manager
            .env
            .iter()
            .map(|(name, value)| Ok((name.clone(), render(value, &vars, dir)?)))
            .collect::<Result<_, ExtensionError>>()?;
        Self::run(commands, &vars, &env, dir)
    }

    pub(crate) fn install(&self, package: &Package, dir: &Path) -> Result<(), ExtensionError> {
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        fs::create_dir_all(dir)?;
        let vars = self.vars(package, dir)?;
        match package.purl.kind.as_str() {
            GITHUB | GENERIC => {
                let entry = package
                    .entry_for(self.platform)
                    .ok_or(ExtensionError::NoBuild)?;
                if package.purl.kind == GITHUB {
                    self.fetch_github(package, entry, &vars, dir)?;
                } else {
                    self.fetch_generic(entry, &vars, dir)?;
                }
            }
            _ => self.manage(package, vars, dir)?,
        }
        fs::write(dir.join(INSTALLED_MARK), &package.purl.version)?;
        Ok(())
    }

    pub(crate) fn command(
        &self,
        package: &Package,
        bin: &str,
        dir: &Path,
    ) -> Result<Vec<String>, ExtensionError> {
        let value = package
            .bins
            .get(bin)
            .ok_or_else(|| ExtensionError::Missing(bin.to_owned()))?;
        let vars = self.vars(package, dir)?;
        let Some((kind, target)) = bin_kind(value) else {
            let relative = render(value, &vars, dir)?;
            return Ok(vec![dir.join(relative).to_string_lossy().into_owned()]);
        };
        let vars = vars.with("bin", target);
        match tool(self.manifests, kind) {
            Some(Tool::Manager(_, manager)) => {
                let template = match (self.platform.os, &manager.windows_bin) {
                    (Os::Windows, Some(windows)) => windows,
                    _ => &manager.bin,
                };
                Ok(vec![render(template, &vars, dir)?])
            }
            Some(Tool::Runner(toolchain, runner)) => {
                let vars = self.with_toolchain(vars, toolchain)?;
                render_all(&runner.command, &vars, dir)
            }
            None if kind == EXEC => Ok(vec![dir.join(target).to_string_lossy().into_owned()]),
            None => Err(ExtensionError::UnknownKind(kind.to_owned())),
        }
    }
}

pub(crate) fn installed_version(dir: &Path) -> Option<String> {
    fs::read_to_string(dir.join(INSTALLED_MARK)).ok()
}
