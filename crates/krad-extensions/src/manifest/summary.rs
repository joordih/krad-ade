use std::fs;
use std::path::Path;

use super::{manifest_files, parse, ManifestFile};
use crate::error::ExtensionError;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ManifestSummary {
    pub languages: Vec<String>,
    pub servers: Vec<String>,
    pub server_runtimes: Vec<String>,
    pub toolchains: Vec<String>,
    pub locators: Vec<String>,
}

impl ManifestSummary {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.languages.is_empty()
            && self.servers.is_empty()
            && self.server_runtimes.is_empty()
            && self.toolchains.is_empty()
            && self.locators.is_empty()
    }

    fn add(&mut self, file: &ManifestFile) {
        self.languages.extend(
            file.languages
                .iter()
                .map(|language| language.id.to_string()),
        );
        self.servers
            .extend(file.servers.iter().map(|server| server.package.clone()));
        self.server_runtimes.extend(
            file.server_runtimes
                .iter()
                .map(|server_runtime| server_runtime.id.to_string()),
        );
        self.toolchains.extend(
            file.toolchain_families
                .iter()
                .map(|family| family.id.to_string()),
        );
        self.locators.extend(
            file.shared_locators
                .iter()
                .map(|locator| locator.id().to_string()),
        );
    }
}

pub(crate) fn summary_of(folder: &Path) -> Result<ManifestSummary, ExtensionError> {
    let mut summary = ManifestSummary::default();
    for path in manifest_files(folder) {
        let text = fs::read_to_string(&path)?;
        summary.add(&parse(&text, &path.to_string_lossy())?);
    }
    Ok(summary)
}
