mod fetch;
mod on_host;

pub(crate) use fetch::fetch_to;
pub(crate) use on_host::Download;
pub(crate) mod output;

use krad_domain::ServerRuntimeId;
use krad_host::{HostCommand, HostLink};
use std::path::Path;
use ureq::Agent;

use crate::error::ExtensionError;
use crate::manifest::{Manifests, PackageManager, Runner, ServerRuntimeManifest};
use crate::platform::{Os, Platform};
use crate::registry::Package;
use crate::server_runtime::FindServerRuntime;
use crate::template::{render, render_all, Vars, SERVER_RUNTIME_DIR_VAR, SERVER_RUNTIME_VAR};

const INSTALLED_MARK: &str = ".krad-installed";
const GITHUB: &str = "github";
const GENERIC: &str = "generic";
const EXEC: &str = "exec";
pub(super) const DESTINATION: char = ':';
const TEMPLATE_START: &str = "{{";
const EXTRA_QUALIFIER: &str = "extra";
const REPOSITORY_QUALIFIER: &str = "repository_url";
pub(crate) const VERSION_SEPARATOR: char = '@';

pub(crate) enum Tool<'a> {
    Manager(&'a ServerRuntimeManifest, &'a PackageManager),
    Runner(&'a ServerRuntimeManifest, &'a Runner),
}

pub(crate) fn tool<'a>(manifests: &'a Manifests, kind: &str) -> Option<Tool<'a>> {
    manifests.server_runtimes.iter().find_map(|server_runtime| {
        server_runtime
            .manager
            .iter()
            .find(|manager| manager.kind == kind)
            .map(|manager| Tool::Manager(server_runtime, manager))
            .or_else(|| {
                server_runtime
                    .runner
                    .iter()
                    .find(|runner| runner.kind == kind)
                    .map(|runner| Tool::Runner(server_runtime, runner))
            })
    })
}

fn bin_kind(value: &str) -> Option<(&str, &str)> {
    if value.starts_with(TEMPLATE_START) {
        return None;
    }
    value.split_once(DESTINATION)
}

pub(crate) fn needed_server_runtimes(
    manifests: &Manifests,
    package: &Package,
    bin: &str,
) -> Vec<ServerRuntimeId> {
    let mut kinds = vec![package.purl.kind.as_str()];
    if let Some((kind, _)) = package.bins.get(bin).and_then(|value| bin_kind(value)) {
        kinds.push(kind);
    }
    let mut needed: Vec<ServerRuntimeId> = Vec::new();
    for kind in kinds {
        let owner = match tool(manifests, kind) {
            Some(Tool::Manager(server_runtime, _) | Tool::Runner(server_runtime, _)) => {
                &server_runtime.id
            }
            None => continue,
        };
        if !needed.contains(owner) {
            needed.push(owner.clone());
        }
    }
    needed
}

pub(crate) struct Installing<'a, F> {
    pub(crate) manifests: &'a Manifests,
    pub(crate) server_runtimes: &'a F,
    pub(crate) platform: Platform,
    pub(crate) agent: &'a Agent,
    pub(crate) link: &'a HostLink,
}

impl<F: FindServerRuntime> Installing<'_, F> {
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

    fn with_server_runtime(
        &self,
        vars: Vars,
        server_runtime: &ServerRuntimeManifest,
    ) -> Result<Vars, ExtensionError> {
        let program = self
            .server_runtimes
            .find(&server_runtime.id)
            .ok_or_else(|| ExtensionError::MissingServerRuntime(server_runtime.id.clone()))?;
        let folder = program
            .parent()
            .map_or_else(String::new, |folder| folder.to_string_lossy().into_owned());
        Ok(vars
            .with(SERVER_RUNTIME_VAR, program.to_string_lossy())
            .with(SERVER_RUNTIME_DIR_VAR, folder))
    }

    fn run(
        &self,
        commands: &[Vec<String>],
        vars: &Vars,
        env: &[(String, String)],
        dir: &Path,
    ) -> Result<(), ExtensionError> {
        let server_runtime_dir = vars
            .get(SERVER_RUNTIME_DIR_VAR)
            .unwrap_or_default()
            .to_owned();
        for command in commands {
            let parts = render_all(command, vars, dir)?;
            let Some((program, args)) = parts.split_first() else {
                continue;
            };
            let mut host_command = HostCommand::new(program);
            host_command.args(args).current_dir(dir).in_login_shell();
            if !server_runtime_dir.is_empty() {
                host_command.prepend_path(&server_runtime_dir);
            }
            for (name, value) in env {
                host_command.env(name, value);
            }
            tracing::debug!(program, "running an install step");
            let output = self.link.output(&host_command)?;
            output::checked(program, &output)?;
        }
        Ok(())
    }

    fn manage(&self, package: &Package, vars: Vars, dir: &Path) -> Result<(), ExtensionError> {
        let kind = &package.purl.kind;
        let Some(Tool::Manager(server_runtime, manager)) = tool(self.manifests, kind) else {
            return Err(ExtensionError::UnknownKind(kind.clone()));
        };
        let vars = self.with_server_runtime(vars, server_runtime)?;
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
        self.run(commands, &vars, &env, dir)
    }

    pub(crate) fn install(&self, package: &Package, dir: &Path) -> Result<(), ExtensionError> {
        if self.link.exists(dir) {
            self.link.remove_dir_all(dir)?;
        }
        self.link.create_dir_all(dir)?;
        if let Err(error) = self.fill(package, dir) {
            if let Err(cleanup) = self.link.remove_dir_all(dir) {
                tracing::warn!(error = %cleanup, "could not remove a failed install");
            }
            return Err(error);
        }
        self.link
            .write(&dir.join(INSTALLED_MARK), package.purl.version.as_bytes())?;
        self.prune_other_versions(dir);
        Ok(())
    }

    fn fill(&self, package: &Package, dir: &Path) -> Result<(), ExtensionError> {
        let vars = self.vars(package, dir)?;
        match package.purl.kind.as_str() {
            GITHUB | GENERIC => {
                let entry = package
                    .entry_for(self.platform)
                    .ok_or(ExtensionError::NoBuild)?;
                if package.purl.kind == GITHUB {
                    self.fetch_github(package, entry, &vars, dir)
                } else {
                    self.fetch_generic(entry, &vars, dir)
                }
            }
            _ => self.manage(package, vars, dir),
        }
    }

    fn prune_other_versions(&self, dir: &Path) {
        let (Some(parent), Some(name)) = (dir.parent(), dir.file_name()) else {
            return;
        };
        let name = name.to_string_lossy();
        let Some(stem) = name.split_once(VERSION_SEPARATOR).map(|(stem, _)| stem) else {
            return;
        };
        let Ok(entries) = self.link.read_dir(parent) else {
            return;
        };
        let older = entries.into_iter().filter(|entry| {
            entry.name != name
                && entry
                    .name
                    .split_once(VERSION_SEPARATOR)
                    .is_some_and(|(other, _)| other == stem)
        });
        for entry in older {
            if let Err(error) = self.link.remove_dir_all(&entry.path) {
                tracing::debug!(error = %error, "an older server version is still in use");
            }
        }
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
            Some(Tool::Runner(server_runtime, runner)) => {
                let vars = self.with_server_runtime(vars, server_runtime)?;
                render_all(&runner.command, &vars, dir)
            }
            None if kind == EXEC => Ok(vec![dir.join(target).to_string_lossy().into_owned()]),
            None => Err(ExtensionError::UnknownKind(kind.to_owned())),
        }
    }
}

pub(crate) fn installed_version(link: &HostLink, dir: &Path) -> Option<String> {
    link.read_to_string(&dir.join(INSTALLED_MARK)).ok()
}
