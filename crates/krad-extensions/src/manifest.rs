use krad_domain::{LanguageCatalog, LanguageDef, ToolchainId};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::ExtensionError;

const BUILTIN: &str = include_str!("../manifests/builtin.toml");
const BUILTIN_NAME: &str = "builtin.toml";
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
    #[serde(default)]
    pub toolchains: Vec<ToolchainId>,
    #[serde(default)]
    pub settings_section: Option<String>,
    #[serde(default)]
    pub settings: Option<toml::Table>,
    #[serde(default)]
    pub command: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct ToolchainDownload {
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
pub struct ToolchainManifest {
    pub id: ToolchainId,
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
    pub(crate) download: Vec<ToolchainDownload>,
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
    #[serde(default, rename = "toolchain")]
    toolchains: Vec<ToolchainManifest>,
}

#[derive(Clone, Debug, Default)]
pub struct Manifests {
    pub languages: LanguageCatalog,
    pub servers: Vec<ServerManifest>,
    pub toolchains: Vec<ToolchainManifest>,
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
        for toolchain in file.toolchains {
            self.toolchains.retain(|known| known.id != toolchain.id);
            self.toolchains.push(toolchain);
        }
    }

    #[must_use]
    pub fn server(&self, package: &str) -> Option<&ServerManifest> {
        self.servers.iter().find(|server| server.package == package)
    }

    #[must_use]
    pub fn toolchain(&self, id: &ToolchainId) -> Option<&ToolchainManifest> {
        self.toolchains.iter().find(|toolchain| toolchain.id == *id)
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
    match parse(BUILTIN, BUILTIN_NAME) {
        Ok(file) => manifests.merge(file),
        Err(error) => problems.push(error),
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
        assert!(manifests.toolchain(&ToolchainId::new("node")).is_some());
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
