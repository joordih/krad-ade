use krad_domain::{LanguageCatalog, LanguageDef, ServerRuntimeId, ToolchainFamilyId};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::ExtensionError;
use crate::toolchains::{ServerToolchain, SharedLocatorManifest, ToolchainFamilyManifest};

const BUILTIN: &str = include_str!("../manifests/builtin.toml");
const BUILTIN_NAME: &str = "builtin.toml";
const TOOLCHAINS: &str = include_str!("../manifests/toolchains.toml");
const TOOLCHAINS_NAME: &str = "toolchains.toml";
const MANIFEST_EXTENSION: &str = "toml";
const EXTENSION_FILE: &str = "extension.toml";

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ServerManifest {
    pub package: String,
    #[serde(default)]
    pub bin: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub probe: Vec<String>,
    #[serde(default)]
    pub path_names: Option<Vec<String>>,
    #[serde(default, alias = "toolchains")]
    pub server_runtimes: Vec<ServerRuntimeId>,
    #[serde(default)]
    pub settings_section: Option<String>,
    #[serde(default)]
    pub settings: Option<toml::Table>,
    #[serde(default)]
    pub command: Option<Vec<String>>,
    #[serde(default)]
    pub toolchain: Option<ServerToolchain>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct RuntimeDownload {
    pub(crate) targets: Vec<String>,
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) sha256_url: Option<String>,
    #[serde(default)]
    pub(crate) sha256_list: Option<String>,
    #[serde(default)]
    pub(crate) run: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct PackageManager {
    pub(crate) kind: String,
    pub(crate) install: Vec<Vec<String>>,
    #[serde(default)]
    pub(crate) windows_install: Option<Vec<Vec<String>>>,
    #[serde(default)]
    pub(crate) install_from_repository: Option<Vec<Vec<String>>>,
    #[serde(default)]
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) bin: String,
    #[serde(default)]
    pub(crate) windows_bin: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct Runner {
    pub(crate) kind: String,
    pub(crate) command: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerRuntimeManifest {
    pub id: ServerRuntimeId,
    pub name: String,
    pub(crate) program: String,
    #[serde(default)]
    pub(crate) windows_program: Option<String>,
    #[serde(default)]
    pub(crate) version_args: Vec<String>,
    #[serde(default)]
    pub(crate) version_after: Option<String>,
    #[serde(default)]
    pub(crate) minimum_version: Option<u32>,
    #[serde(default)]
    pub(crate) home_vars: Vec<String>,
    #[serde(default)]
    pub(crate) launch_home_var: Option<String>,
    #[serde(default)]
    pub(crate) search: Vec<String>,
    #[serde(default)]
    pub(crate) download: Vec<RuntimeDownload>,
    #[serde(default)]
    pub(crate) manager: Vec<PackageManager>,
    #[serde(default)]
    pub(crate) runner: Vec<Runner>,
}

#[derive(Debug, Default, Deserialize)]
struct ManifestFile {
    #[serde(default, rename = "language")]
    languages: Vec<LanguageDef>,
    #[serde(default, rename = "server")]
    servers: Vec<ServerManifest>,
    #[serde(default, rename = "server_runtime", alias = "toolchain")]
    server_runtimes: Vec<ServerRuntimeManifest>,
    #[serde(default, rename = "toolchains")]
    toolchain_families: Vec<ToolchainFamilyManifest>,
    #[serde(default, rename = "locator")]
    shared_locators: Vec<SharedLocatorManifest>,
}

#[derive(Clone, Debug, Default)]
pub struct Manifests {
    pub languages: LanguageCatalog,
    pub servers: Vec<ServerManifest>,
    pub server_runtimes: Vec<ServerRuntimeManifest>,
    pub toolchain_families: Vec<ToolchainFamilyManifest>,
    pub shared_locators: Vec<SharedLocatorManifest>,
}

impl Manifests {
    fn merge(&mut self, file: ManifestFile) {
        for language in file.languages {
            self.languages.add(language);
        }
        for server in file.servers {
            self.servers.retain(|known| known.package != server.package);
            self.servers.push(server);
        }
        for server_runtime in file.server_runtimes {
            self.server_runtimes
                .retain(|known| known.id != server_runtime.id);
            self.server_runtimes.push(server_runtime);
        }
        for family in file.toolchain_families {
            self.toolchain_families
                .retain(|known| known.id != family.id);
            self.toolchain_families.push(family);
        }
        for locator in file.shared_locators {
            self.shared_locators
                .retain(|known| known.id() != locator.id());
            self.shared_locators.push(locator);
        }
    }

    #[must_use]
    pub fn toolchain_family(&self, id: &ToolchainFamilyId) -> Option<&ToolchainFamilyManifest> {
        self.toolchain_families
            .iter()
            .find(|family| family.id == *id)
    }

    #[must_use]
    pub fn toolchain_families_of(&self, language: &str) -> Vec<&ToolchainFamilyManifest> {
        self.toolchain_families
            .iter()
            .filter(|family| family.covers(language))
            .collect()
    }

    #[must_use]
    pub fn server(&self, package: &str) -> Option<&ServerManifest> {
        self.servers.iter().find(|server| server.package == package)
    }

    #[must_use]
    pub fn server_runtime(&self, id: &ServerRuntimeId) -> Option<&ServerRuntimeManifest> {
        self.server_runtimes
            .iter()
            .find(|server_runtime| server_runtime.id == *id)
    }
}

fn parse(text: &str, file: &str) -> Result<ManifestFile, ExtensionError> {
    toml::from_str(text).map_err(|error| ExtensionError::Manifest {
        file: file.to_owned(),
        reason: error.to_string(),
    })
}

fn manifest_files(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .flat_map(|path| {
            if path.is_dir() {
                manifest_files(&path)
            } else {
                vec![path]
            }
        })
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == MANIFEST_EXTENSION)
                && path.file_name().is_some_and(|name| name != EXTENSION_FILE)
        })
        .collect();
    files.sort();
    files
}

#[must_use]
pub fn builtin_catalog() -> LanguageCatalog {
    parse(BUILTIN, BUILTIN_NAME).map_or_else(
        |_| LanguageCatalog::default(),
        |file| LanguageCatalog::new(file.languages),
    )
}

pub(crate) struct Loaded {
    pub(crate) manifests: Manifests,
    pub(crate) problems: Vec<ExtensionError>,
}

pub(crate) fn load(folders: &[PathBuf]) -> Loaded {
    let mut manifests = Manifests::default();
    let mut problems = Vec::new();
    for (text, name) in [(BUILTIN, BUILTIN_NAME), (TOOLCHAINS, TOOLCHAINS_NAME)] {
        match parse(text, name) {
            Ok(file) => manifests.merge(file),
            Err(error) => problems.push(error),
        }
    }
    for path in folders.iter().flat_map(|folder| manifest_files(folder)) {
        let read = fs::read_to_string(&path)
            .map_err(ExtensionError::from)
            .and_then(|text| parse(&text, &path.to_string_lossy()));
        match read {
            Ok(file) => manifests.merge(file),
            Err(error) => problems.push(error),
        }
    }
    Loaded {
        manifests,
        problems,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::LanguageId;

    #[test]
    fn the_builtin_manifest_reads_and_names_a_server_for_rust() {
        let loaded = load(&[]);
        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        let manifests = loaded.manifests;
        let rust = manifests.languages.get(&LanguageId::new("rust"));
        assert_eq!(
            rust.and_then(|rust| rust.server.as_deref()),
            Some("rust-analyzer")
        );
        assert!(manifests
            .server("jdtls")
            .is_some_and(|jdtls| jdtls.command.is_some()));
        assert!(manifests
            .server_runtime(&ServerRuntimeId::new("node"))
            .is_some());
    }

    #[test]
    fn every_toolchain_family_and_hand_off_is_complete() -> Result<(), String> {
        use crate::toolchains::manifest::LocatorSpec;

        let loaded = load(&[]);
        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        let manifests = loaded.manifests;
        for id in ["python", "typescript", "node", "go", "java", "rust", "ruby"] {
            let family = manifests
                .toolchain_family(&ToolchainFamilyId::new(id))
                .ok_or(format!("no {id} toolchain family"))?;
            assert!(!family.locators.is_empty(), "{id} has no locators");
            assert!(!family.markers.is_empty(), "{id} has no project markers");
            for locator in &family.locators {
                if let LocatorSpec::Shared(shared) = locator {
                    assert!(
                        manifests
                            .shared_locators
                            .iter()
                            .any(|known| *known.id() == shared.uses),
                        "{id} uses an unknown shared locator {}",
                        shared.uses
                    );
                }
            }
        }
        assert_eq!(manifests.toolchain_families_of("tsx").len(), 2);
        for server in &manifests.servers {
            if let Some(toolchain) = &server.toolchain {
                assert!(
                    manifests.toolchain_family(&toolchain.family).is_some(),
                    "{} hands off to an unknown family",
                    server.package
                );
            }
        }
        assert!(manifests
            .server("basedpyright")
            .and_then(|server| server.toolchain.as_ref())
            .is_some_and(|toolchain| toolchain.settings.contains_key("python.pythonPath")));
        Ok(())
    }

    #[test]
    fn legacy_toolchain_keys_still_describe_server_runtimes() -> Result<(), ExtensionError> {
        let legacy = "[[server]]\npackage = \"zls\"\ntoolchains = [\"zig\"]\n\n[[toolchain]]\nid = \"zig\"\nname = \"Zig\"\nprogram = \"zig\"\n";
        let mut manifests = Manifests::default();
        manifests.merge(parse(legacy, "legacy.toml")?);
        assert_eq!(
            manifests
                .server("zls")
                .map(|server| server.server_runtimes.clone()),
            Some(vec![ServerRuntimeId::new("zig")])
        );
        assert!(manifests
            .server_runtime(&ServerRuntimeId::new("zig"))
            .is_some());
        Ok(())
    }

    #[test]
    fn a_user_manifest_adds_a_language_and_overrides_a_server() -> Result<(), ExtensionError> {
        let folder = std::env::temp_dir().join(format!("krad-manifests-{}", std::process::id()));
        fs::create_dir_all(&folder)?;
        fs::write(
            folder.join("zig.toml"),
            "[[language]]\nid = \"zig\"\nname = \"Zig\"\nextensions = [\"zig\"]\nserver = \"zls\"\n\n[[server]]\npackage = \"rust-analyzer\"\nargs = [\"--log\"]\n",
        )?;
        let manifests = load(std::slice::from_ref(&folder)).manifests;
        assert!(manifests
            .languages
            .for_path(Path::new("main.zig"))
            .is_some());
        assert_eq!(
            manifests
                .server("rust-analyzer")
                .map(|server| server.args.clone()),
            Some(vec!["--log".to_owned()])
        );
        fs::remove_dir_all(&folder)?;
        Ok(())
    }
}
