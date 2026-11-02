use krad_domain::{LanguageCatalog, LanguageDef, ServerLaunch, ToolchainId, ToolchainPaths};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use ureq::Agent;

use crate::error::ExtensionError;
use crate::http;
use crate::manifest::{load, Manifests, ServerManifest};
use crate::package::{installed_version, needed_toolchains, Installing};
use crate::path_lookup::find_on_path;
use crate::platform::Platform;
use crate::registry::{self, Package, Registry};
use crate::template::{render_all, Vars};
use crate::toolchain::Toolchains;

const LANGUAGES_DIR: &str = "languages";
const PACKAGES_DIR: &str = "packages";
const WORKSPACES_DIR: &str = "workspaces";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServerPlan {
    pub package: String,
    bin: String,
    pub toolchains: Vec<ToolchainId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingToolchain {
    pub id: ToolchainId,
    pub name: String,
    pub installable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolchainStatus {
    pub id: ToolchainId,
    pub name: String,
    pub found: Option<PathBuf>,
    pub chosen: bool,
    pub installable: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerConfig {
    pub section: Option<String>,
    pub options: Option<Value>,
}

impl ServerConfig {
    #[must_use]
    pub fn sections(&self) -> Value {
        match (&self.section, &self.options) {
            (Some(section), Some(options)) => Value::Object(serde_json::Map::from_iter([(
                section.clone(),
                options.clone(),
            )])),
            _ => Value::Null,
        }
    }
}

pub struct LanguageTools {
    root: PathBuf,
    platform: Platform,
    agent: Agent,
    manifests: Manifests,
    registry: Registry,
    problems: Vec<String>,
}

fn flattened(root: &Path) -> String {
    root.to_string_lossy()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

impl LanguageTools {
    /// # Errors
    ///
    /// Returns [`ExtensionError::Unsupported`] on a platform the engine does not know.
    pub fn load(config_dir: &Path, manifest_folders: &[PathBuf]) -> Result<Self, ExtensionError> {
        let platform = Platform::current().ok_or(ExtensionError::Unsupported)?;
        let root = config_dir.join(LANGUAGES_DIR);
        let loaded = load(manifest_folders);
        Ok(Self {
            registry: registry::cached(&root),
            root,
            platform,
            agent: http::agent(),
            manifests: loaded.manifests,
            problems: loaded.problems.iter().map(ToString::to_string).collect(),
        })
    }

    #[must_use]
    pub const fn catalog(&self) -> &LanguageCatalog {
        &self.manifests.languages
    }

    #[must_use]
    pub fn problems(&self) -> &[String] {
        &self.problems
    }

    #[must_use]
    pub fn registry_is_stale(&self) -> bool {
        self.registry.is_empty() || registry::is_stale(&self.root)
    }

    /// # Errors
    ///
    /// Returns [`ExtensionError`] when the registry cannot be fetched or verified.
    pub fn refreshed_registry(&self) -> Result<Registry, ExtensionError> {
        registry::refresh(&self.agent, &self.root, self.platform)
    }

    #[must_use]
    pub fn with_registry(mut self, registry: Registry) -> Self {
        self.registry = registry;
        self
    }

    fn toolchains<'a>(&'a self, chosen: &'a ToolchainPaths) -> Toolchains<'a> {
        Toolchains {
            manifests: &self.manifests,
            root: &self.root,
            platform: self.platform,
            chosen,
        }
    }

    fn manifest(&self, plan: &ServerPlan) -> ServerManifest {
        self.manifests
            .server(&plan.package)
            .cloned()
            .unwrap_or_else(|| ServerManifest {
                package: plan.package.clone(),
                ..ServerManifest::default()
            })
    }

    #[must_use]
    pub fn plan(&self, language: &LanguageDef) -> Option<ServerPlan> {
        let package_name = language.server.clone()?;
        let manifest = self
            .manifests
            .server(&package_name)
            .cloned()
            .unwrap_or_default();
        let package = self.registry.package(&package_name);
        let bin = manifest.bin.clone().unwrap_or_else(|| {
            package
                .and_then(|package| {
                    package
                        .bins
                        .keys()
                        .find(|name| **name == package_name)
                        .or_else(|| package.bins.keys().next())
                        .cloned()
                })
                .unwrap_or_else(|| package_name.clone())
        });
        let mut toolchains = manifest.toolchains.clone();
        let kinds = package
            .filter(|_| manifest.command.is_none())
            .map(|package| needed_toolchains(&self.manifests, package, &bin));
        for id in language
            .toolchains
            .iter()
            .cloned()
            .chain(kinds.into_iter().flatten())
        {
            if !toolchains.contains(&id) {
                toolchains.push(id);
            }
        }
        Some(ServerPlan {
            package: package_name,
            bin,
            toolchains,
        })
    }

    #[must_use]
    pub fn toolchain_name(&self, id: &ToolchainId) -> String {
        self.manifests
            .toolchain(id)
            .map_or_else(|| id.to_string(), |toolchain| toolchain.name.clone())
    }

    #[must_use]
    pub fn missing_toolchain(
        &self,
        plan: &ServerPlan,
        chosen: &ToolchainPaths,
    ) -> Option<MissingToolchain> {
        let toolchains = self.toolchains(chosen);
        plan.toolchains
            .iter()
            .find(|id| toolchains.find(id).is_none())
            .map(|id| MissingToolchain {
                id: id.clone(),
                name: self.toolchain_name(id),
                installable: toolchains.installable(id),
            })
    }

    /// # Errors
    ///
    /// Returns [`ExtensionError`] when the toolchain cannot be fetched, verified or set up.
    pub fn install_toolchain(
        &self,
        id: &ToolchainId,
        chosen: &ToolchainPaths,
    ) -> Result<(), ExtensionError> {
        self.toolchains(chosen).install(id, &self.agent).map(drop)
    }

    #[must_use]
    pub fn toolchain_statuses(&self, chosen: &ToolchainPaths) -> Vec<ToolchainStatus> {
        let toolchains = self.toolchains(chosen);
        self.manifests
            .toolchains
            .iter()
            .map(|toolchain| ToolchainStatus {
                id: toolchain.id.clone(),
                name: toolchain.name.clone(),
                found: toolchains.find(&toolchain.id),
                chosen: chosen.get(&toolchain.id).is_some(),
                installable: toolchains.installable(&toolchain.id),
            })
            .collect()
    }

    #[must_use]
    pub fn accepts_folder(&self, id: &ToolchainId, folder: &Path, chosen: &ToolchainPaths) -> bool {
        self.toolchains(chosen).accepts(id, folder)
    }

    #[must_use]
    pub fn config(&self, plan: &ServerPlan) -> ServerConfig {
        let manifest = self.manifest(plan);
        ServerConfig {
            section: manifest.settings_section,
            options: manifest
                .settings
                .and_then(|settings| serde_json::to_value(settings).ok()),
        }
    }

    fn package(&self, plan: &ServerPlan) -> Result<&Package, ExtensionError> {
        self.registry
            .package(&plan.package)
            .ok_or_else(|| ExtensionError::UnknownPackage(plan.package.clone()))
    }

    fn package_dir(&self, plan: &ServerPlan) -> PathBuf {
        self.root.join(PACKAGES_DIR).join(&plan.package)
    }

    fn found_toolchains(
        &self,
        plan: &ServerPlan,
        chosen: &ToolchainPaths,
    ) -> Option<Vec<(ToolchainId, PathBuf)>> {
        let toolchains = self.toolchains(chosen);
        plan.toolchains
            .iter()
            .map(|id| toolchains.find(id).map(|program| (id.clone(), program)))
            .collect()
    }

    fn on_path(&self, plan: &ServerPlan, root: &Path) -> Option<PathBuf> {
        let manifest = self.manifest(plan);
        let names = manifest
            .path_names
            .clone()
            .unwrap_or_else(|| vec![plan.bin.clone()]);
        names
            .iter()
            .filter_map(|name| find_on_path(name))
            .find(|program| {
                manifest.probe.is_empty()
                    || Command::new(program)
                        .args(&manifest.probe)
                        .current_dir(root)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .is_ok_and(|status| status.success())
            })
    }

    fn launch(
        &self,
        plan: &ServerPlan,
        root: &Path,
        chosen: &ToolchainPaths,
        found: &[(ToolchainId, PathBuf)],
    ) -> Result<ServerLaunch, ExtensionError> {
        let manifest = self.manifest(plan);
        let toolchains = self.toolchains(chosen);
        let env = toolchains.launch_env(found);
        if let Some(program) = self.on_path(plan, root) {
            return Ok(ServerLaunch {
                program,
                args: manifest.args,
                env,
            });
        }
        let package = self.package(plan)?;
        let dir = self.package_dir(plan);
        let installing = Installing {
            manifests: &self.manifests,
            toolchains: &toolchains,
            platform: self.platform,
            agent: &self.agent,
        };
        let command = if let Some(template) = &manifest.command {
            let mut vars: Vars = installing.vars(package, &dir)?;
            vars.set(
                "workspace",
                self.root
                    .join(WORKSPACES_DIR)
                    .join(flattened(root))
                    .to_string_lossy(),
            );
            vars.set("root", root.to_string_lossy());
            for (id, program) in found {
                vars.set(format!("toolchain.{id}"), program.to_string_lossy());
            }
            render_all(template, &vars, &dir)?
        } else {
            let mut command = installing.command(package, &plan.bin, &dir)?;
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

    #[must_use]
    pub fn locate(
        &self,
        plan: &ServerPlan,
        root: &Path,
        chosen: &ToolchainPaths,
    ) -> Option<ServerLaunch> {
        let found = self.found_toolchains(plan, chosen)?;
        if self.on_path(plan, root).is_none() {
            let installed = installed_version(&self.package_dir(plan))?;
            let current = self
                .registry
                .package(&plan.package)
                .is_none_or(|package| package.purl.version == installed);
            if !current {
                return None;
            }
        }
        self.launch(plan, root, chosen, &found).ok()
    }

    /// # Errors
    ///
    /// Returns [`ExtensionError`] when a toolchain is missing or the package cannot be installed.
    pub fn install(
        &self,
        plan: &ServerPlan,
        root: &Path,
        chosen: &ToolchainPaths,
    ) -> Result<ServerLaunch, ExtensionError> {
        let Some(found) = self.found_toolchains(plan, chosen) else {
            let missing = self
                .missing_toolchain(plan, chosen)
                .map_or_else(|| ToolchainId::new(""), |missing| missing.id);
            return Err(ExtensionError::MissingToolchain(missing));
        };
        let package = self.package(plan)?;
        let toolchains = self.toolchains(chosen);
        Installing {
            manifests: &self.manifests,
            toolchains: &toolchains,
            platform: self.platform,
            agent: &self.agent,
        }
        .install(package, &self.package_dir(plan))?;
        self.launch(plan, root, chosen, &found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::LanguageId;

    #[test]
    fn a_plan_gathers_the_toolchains_of_the_server_and_its_package_kind(
    ) -> Result<(), ExtensionError> {
        let config = std::env::temp_dir().join(format!("krad-tools-{}", std::process::id()));
        let registry = Registry::from_json(
            r#"[{"name":"vtsls","languages":["TypeScript"],
                "source":{"id":"pkg:npm/%40vtsls/language-server@0.3.0"},
                "bin":{"vtsls":"npm:vtsls"}}]"#,
        )?;
        let tools = LanguageTools::load(&config, &[])?.with_registry(registry);
        let typescript = tools
            .catalog()
            .get(&LanguageId::new("typescript"))
            .cloned()
            .ok_or(ExtensionError::NoBuild)?;
        let plan = tools.plan(&typescript).ok_or(ExtensionError::NoBuild)?;
        assert_eq!(plan.package, "vtsls");
        assert_eq!(plan.toolchains, [ToolchainId::new("node")]);
        Ok(())
    }
}
