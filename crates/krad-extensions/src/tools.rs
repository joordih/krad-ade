use krad_domain::{
    LanguageCatalog, LanguageDef, ServerLaunch, ServerRuntimeId, ServerRuntimePaths,
    VersionConstraint,
};
use serde_json::Value;
use std::path::{Path, PathBuf};
use ureq::Agent;

use crate::error::ExtensionError;
use crate::http;
use crate::manifest::{load, Manifests, ServerManifest, ServerScope};
use crate::package::needed_server_runtimes;
use crate::platform::Platform;
use crate::registry::{self, Package, Registry};
use crate::runtime_requirements::{RequirementCache, RuntimeRequirements};
use crate::server_runtime::ServerRuntimes;

mod project_toolchains;
mod site;

const LANGUAGES_DIR: &str = "languages";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServerPlan {
    pub package: String,
    bin: String,
    pub server_runtimes: Vec<ServerRuntimeId>,
    pub scope: ServerScope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingServerRuntime {
    pub id: ServerRuntimeId,
    pub name: String,
    pub installable: bool,
    pub needs: Option<VersionConstraint>,
    pub outdated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerRuntimeStatus {
    pub id: ServerRuntimeId,
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
    requirement_cache: RequirementCache,
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
            registry: registry::cached(&root).with_manifest_sources(&loaded.manifests.servers),
            requirement_cache: RequirementCache::load(&root),
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
        self.registry = registry.with_manifest_sources(&self.manifests.servers);
        self
    }

    fn server_runtimes<'a>(
        &'a self,
        chosen: &'a ServerRuntimePaths,
        requirements: &'a RuntimeRequirements,
    ) -> ServerRuntimes<'a> {
        ServerRuntimes {
            manifests: &self.manifests,
            root: &self.root,
            platform: self.platform,
            chosen,
            requirements,
        }
    }

    pub(crate) fn plan_requirements(&self, plan: &ServerPlan) -> RuntimeRequirements {
        self.registry
            .package(&plan.package)
            .map_or_else(RuntimeRequirements::new, |package| {
                self.requirement_cache
                    .requirements(&self.agent, &self.manifests, &package.purl)
            })
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
        let mut server_runtimes = manifest.server_runtimes.clone();
        let kinds = package
            .filter(|_| manifest.command.is_none())
            .map(|package| needed_server_runtimes(&self.manifests, package, &bin));
        for id in language
            .server_runtimes
            .iter()
            .cloned()
            .chain(kinds.into_iter().flatten())
        {
            if !server_runtimes.contains(&id) {
                server_runtimes.push(id);
            }
        }
        Some(ServerPlan {
            package: package_name,
            bin,
            server_runtimes,
            scope: manifest.scope,
        })
    }

    #[must_use]
    pub fn server_runtime_name(&self, id: &ServerRuntimeId) -> String {
        self.manifests.server_runtime(id).map_or_else(
            || id.to_string(),
            |server_runtime| server_runtime.name.clone(),
        )
    }

    #[must_use]
    pub fn missing_server_runtime(
        &self,
        plan: &ServerPlan,
        chosen: &ServerRuntimePaths,
    ) -> Option<MissingServerRuntime> {
        let requirements = self.plan_requirements(plan);
        let server_runtimes = self.server_runtimes(chosen, &requirements);
        let unconstrained = RuntimeRequirements::new();
        let any_version = self.server_runtimes(chosen, &unconstrained);
        plan.server_runtimes
            .iter()
            .find(|id| server_runtimes.find(id).is_none())
            .map(|id| MissingServerRuntime {
                id: id.clone(),
                name: self.server_runtime_name(id),
                installable: server_runtimes.installable(id),
                needs: requirements.get(id).cloned(),
                outdated: any_version.find(id).is_some(),
            })
    }

    /// # Errors
    ///
    /// Returns [`ExtensionError`] when the server runtime cannot be fetched, verified or set up.
    pub fn install_server_runtime(
        &self,
        id: &ServerRuntimeId,
        chosen: &ServerRuntimePaths,
    ) -> Result<(), ExtensionError> {
        self.server_runtimes(chosen, &RuntimeRequirements::new())
            .install(id, &self.agent)
            .map(drop)
    }

    #[must_use]
    pub fn server_runtime_statuses(&self, chosen: &ServerRuntimePaths) -> Vec<ServerRuntimeStatus> {
        let unconstrained = RuntimeRequirements::new();
        let server_runtimes = self.server_runtimes(chosen, &unconstrained);
        self.manifests
            .server_runtimes
            .iter()
            .map(|server_runtime| ServerRuntimeStatus {
                id: server_runtime.id.clone(),
                name: server_runtime.name.clone(),
                found: server_runtimes.find(&server_runtime.id),
                chosen: chosen.get(&server_runtime.id).is_some(),
                installable: server_runtimes.installable(&server_runtime.id),
            })
            .collect()
    }

    #[must_use]
    pub fn accepts_folder(
        &self,
        id: &ServerRuntimeId,
        folder: &Path,
        chosen: &ServerRuntimePaths,
    ) -> bool {
        self.server_runtimes(chosen, &RuntimeRequirements::new())
            .accepts(id, folder)
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

    #[must_use]
    pub fn locate(
        &self,
        plan: &ServerPlan,
        root: &Path,
        chosen: &ServerRuntimePaths,
    ) -> Option<ServerLaunch> {
        let requirements = self.plan_requirements(plan);
        let server_runtimes = self.server_runtimes(chosen, &requirements);
        self.locate_at(&self.local_site(&server_runtimes), plan, root)
    }

    /// # Errors
    ///
    /// Returns [`ExtensionError`] when a server runtime is missing or the package cannot be installed.
    pub fn install(
        &self,
        plan: &ServerPlan,
        root: &Path,
        chosen: &ServerRuntimePaths,
    ) -> Result<ServerLaunch, ExtensionError> {
        let requirements = self.plan_requirements(plan);
        let server_runtimes = self.server_runtimes(chosen, &requirements);
        let site = self.local_site(&server_runtimes);
        self.install_at(&site, plan, root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::LanguageId;

    #[test]
    fn a_plan_gathers_the_server_runtimes_of_the_server_and_its_package_kind(
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
        assert_eq!(plan.server_runtimes, [ServerRuntimeId::new("node")]);
        Ok(())
    }
}
