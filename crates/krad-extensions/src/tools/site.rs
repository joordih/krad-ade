use krad_domain::{ServerLaunch, ServerRuntimeId, ServerRuntimePaths};
use krad_host::{FileMode, HostLink, HostPlatform};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{flattened, LanguageTools, MissingServerRuntime, ServerPlan};
use crate::error::ExtensionError;
use crate::host_tools::{find_on_host, runs_on_host, HostServerRuntimes};
use crate::package::{installed_version, Installing, VERSION_SEPARATOR};
use crate::path_lookup::find_on_path;
use crate::platform::Platform;
use crate::server_runtime::{FindServerRuntime, SERVER_RUNTIMES_DIR};
use crate::template::{render_all, Vars, SERVER_RUNTIME_PREFIX};

const PACKAGES_DIR: &str = "packages";
const WORKSPACES_DIR: &str = "workspaces";

pub(super) struct Site<'a, F> {
    link: &'a HostLink,
    root: PathBuf,
    platform: Platform,
    finder: &'a F,
}

impl LanguageTools {
    pub(super) fn local_site<'a, F>(&self, finder: &'a F) -> Site<'a, F> {
        Site {
            link: &HostLink::Local,
            root: self.root.clone(),
            platform: self.platform,
            finder,
        }
    }

    fn host_site<'a>(
        link: &'a HostLink,
        platform: HostPlatform,
        finder: &'a HostServerRuntimes<'a>,
    ) -> Option<Site<'a, HostServerRuntimes<'a>>> {
        let HostLink::Ssh(ssh) = link else {
            return None;
        };
        Some(Site {
            link,
            root: ssh.layout()?.languages_dir(),
            platform: platform.into(),
            finder,
        })
    }

    fn host_server_runtimes<'a>(&'a self, link: &'a HostLink) -> HostServerRuntimes<'a> {
        let managed = match link {
            HostLink::Ssh(ssh) => ssh
                .layout()
                .map(|layout| layout.languages_dir().join(SERVER_RUNTIMES_DIR)),
            HostLink::Local => None,
        };
        HostServerRuntimes {
            manifests: &self.manifests,
            link,
            managed,
        }
    }

    /// # Errors
    ///
    /// Returns [`ExtensionError`] when the server runtime cannot be fetched or set up on the host.
    pub fn install_server_runtime_on(
        &self,
        link: &HostLink,
        platform: Option<HostPlatform>,
        id: &ServerRuntimeId,
        chosen: &ServerRuntimePaths,
    ) -> Result<(), ExtensionError> {
        if link.is_local() {
            return self.install_server_runtime(id, chosen);
        }
        let platform = platform.ok_or(ExtensionError::Host(krad_host::HostError::NotConnected))?;
        self.host_server_runtimes(link)
            .install(id, platform.into(), &self.agent)
            .map(drop)
    }

    #[must_use]
    pub fn locate_on(
        &self,
        link: &HostLink,
        platform: Option<HostPlatform>,
        plan: &ServerPlan,
        root: &Path,
        chosen: &ServerRuntimePaths,
    ) -> Option<ServerLaunch> {
        if link.is_local() {
            return self.locate(plan, root, chosen);
        }
        let platform = platform?;
        let finder = self.host_server_runtimes(link);
        let site = Self::host_site(link, platform, &finder)?;
        self.locate_at(&site, plan, root)
    }

    /// # Errors
    ///
    /// Returns [`ExtensionError`] when a server runtime is missing on the host or the package cannot be
    /// installed there.
    pub fn install_on(
        &self,
        link: &HostLink,
        platform: Option<HostPlatform>,
        plan: &ServerPlan,
        root: &Path,
        chosen: &ServerRuntimePaths,
    ) -> Result<ServerLaunch, ExtensionError> {
        if link.is_local() {
            return self.install(plan, root, chosen);
        }
        let platform = platform.ok_or(ExtensionError::Host(krad_host::HostError::NotConnected))?;
        let finder = self.host_server_runtimes(link);
        let site = Self::host_site(link, platform, &finder)
            .ok_or(ExtensionError::Host(krad_host::HostError::NotConnected))?;
        self.install_at(&site, plan, root)
    }

    #[must_use]
    pub fn missing_server_runtime_on(
        &self,
        link: &HostLink,
        platform: Option<HostPlatform>,
        plan: &ServerPlan,
        chosen: &ServerRuntimePaths,
    ) -> Option<MissingServerRuntime> {
        if link.is_local() {
            return self.missing_server_runtime(plan, chosen);
        }
        let platform: Platform = platform?.into();
        let finder = self.host_server_runtimes(link);
        plan.server_runtimes
            .iter()
            .find(|id| finder.find(id).is_none())
            .map(|id| MissingServerRuntime {
                id: id.clone(),
                name: self.server_runtime_name(id),
                installable: finder.installable(id, platform),
            })
    }

    fn package_dir_at<F>(&self, site: &Site<'_, F>, plan: &ServerPlan) -> PathBuf {
        let packages = site.root.join(PACKAGES_DIR);
        self.registry.package(&plan.package).map_or_else(
            || packages.join(&plan.package),
            |package| {
                packages.join(format!(
                    "{}{VERSION_SEPARATOR}{}",
                    plan.package, package.purl.version
                ))
            },
        )
    }

    fn installed_dir_at<F>(&self, site: &Site<'_, F>, plan: &ServerPlan) -> Option<PathBuf> {
        let current = self
            .registry
            .package(&plan.package)
            .map(|package| package.purl.version.clone());
        let versioned = self.package_dir_at(site, plan);
        let legacy = site.root.join(PACKAGES_DIR).join(&plan.package);
        [versioned, legacy].into_iter().find(|dir| {
            installed_version(site.link, dir).is_some_and(|installed| {
                current.as_ref().is_none_or(|current| *current == installed)
            })
        })
    }

    fn found_server_runtimes_at<F: FindServerRuntime>(
        site: &Site<'_, F>,
        plan: &ServerPlan,
    ) -> Option<Vec<(ServerRuntimeId, PathBuf)>> {
        plan.server_runtimes
            .iter()
            .map(|id| site.finder.find(id).map(|program| (id.clone(), program)))
            .collect()
    }

    fn on_path_at<F>(&self, site: &Site<'_, F>, plan: &ServerPlan, root: &Path) -> Option<PathBuf> {
        let manifest = self.manifest(plan);
        let names = manifest
            .path_names
            .clone()
            .unwrap_or_else(|| vec![plan.bin.clone()]);
        names.iter().find_map(|name| {
            let program = match site.link {
                HostLink::Local => find_on_path(name)?,
                HostLink::Ssh(_) => find_on_host(site.link, name)?,
            };
            let probed = manifest.probe.is_empty()
                || match site.link {
                    HostLink::Local => Command::new(&program)
                        .args(&manifest.probe)
                        .current_dir(root)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .is_ok_and(|status| status.success()),
                    HostLink::Ssh(_) => runs_on_host(site.link, &program, &manifest.probe, root),
                };
            probed.then_some(program)
        })
    }

    fn installing<'a, F>(&'a self, site: &'a Site<'a, F>) -> Installing<'a, F> {
        Installing {
            manifests: &self.manifests,
            server_runtimes: site.finder,
            platform: site.platform,
            agent: &self.agent,
            link: site.link,
        }
    }

    fn launch_at<F: FindServerRuntime>(
        &self,
        site: &Site<'_, F>,
        plan: &ServerPlan,
        root: &Path,
        found: &[(ServerRuntimeId, PathBuf)],
        dir: &Path,
    ) -> Result<ServerLaunch, ExtensionError> {
        let manifest = self.manifest(plan);
        let env = site.finder.launch_env(found);
        if let Some(program) = self.on_path_at(site, plan, root) {
            return Ok(ServerLaunch {
                program,
                args: manifest.args,
                env,
            });
        }
        let package = self.package(plan)?;
        let installing = self.installing(site);
        let command = if let Some(template) = &manifest.command {
            let mut vars: Vars = installing.vars(package, dir)?;
            vars.set(
                "workspace",
                site.root
                    .join(WORKSPACES_DIR)
                    .join(flattened(root))
                    .to_string_lossy(),
            );
            vars.set("root", root.to_string_lossy());
            for (id, program) in found {
                vars.set(
                    format!("{SERVER_RUNTIME_PREFIX}{id}"),
                    program.to_string_lossy(),
                );
            }
            render_all(template, &vars, dir)?
        } else {
            let mut command = installing.command(package, &plan.bin, dir)?;
            command.extend(manifest.args);
            command
        };
        let (program, args) = command
            .split_first()
            .ok_or_else(|| ExtensionError::Missing(plan.bin.clone()))?;
        Ok(ServerLaunch {
            program: PathBuf::from(program),
            args: args.to_vec(),
            env,
        })
    }

    pub(super) fn locate_at<F: FindServerRuntime>(
        &self,
        site: &Site<'_, F>,
        plan: &ServerPlan,
        root: &Path,
    ) -> Option<ServerLaunch> {
        let found = Self::found_server_runtimes_at(site, plan)?;
        let dir = if self.on_path_at(site, plan, root).is_none() {
            self.installed_dir_at(site, plan)?
        } else {
            self.package_dir_at(site, plan)
        };
        self.launch_at(site, plan, root, &found, &dir).ok()
    }

    pub(super) fn install_at<F: FindServerRuntime>(
        &self,
        site: &Site<'_, F>,
        plan: &ServerPlan,
        root: &Path,
    ) -> Result<ServerLaunch, ExtensionError> {
        let Some(found) = Self::found_server_runtimes_at(site, plan) else {
            let missing = plan
                .server_runtimes
                .iter()
                .find(|id| site.finder.find(id).is_none())
                .cloned();
            return Err(missing.map_or_else(
                || ExtensionError::Missing(plan.bin.clone()),
                ExtensionError::MissingServerRuntime,
            ));
        };
        let package = self.package(plan)?;
        let dir = self.package_dir_at(site, plan);
        self.installing(site).install(package, &dir)?;
        let launch = self.launch_at(site, plan, root, &found, &dir)?;
        if !site.link.is_local() && launch.program.starts_with(&dir) {
            site.link
                .set_permissions(&launch.program, FileMode::EXECUTABLE)?;
        }
        Ok(launch)
    }
}
