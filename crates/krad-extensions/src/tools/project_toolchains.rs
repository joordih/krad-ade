use krad_domain::{LanguageDef, ToolchainFamilyId};

use super::{LanguageTools, ServerPlan};
use crate::toolchains::{ServerToolchain, SharedLocatorManifest, ToolchainFamilyManifest};

impl LanguageTools {
    #[must_use]
    pub fn toolchain_families(&self, language: &LanguageDef) -> Vec<&ToolchainFamilyManifest> {
        self.manifests.toolchain_families_of(language.id.as_str())
    }

    #[must_use]
    pub fn toolchain_family(&self, id: &ToolchainFamilyId) -> Option<&ToolchainFamilyManifest> {
        self.manifests.toolchain_family(id)
    }

    #[must_use]
    pub fn all_toolchain_families(&self) -> &[ToolchainFamilyManifest] {
        &self.manifests.toolchain_families
    }

    #[must_use]
    pub fn shared_locators(&self) -> &[SharedLocatorManifest] {
        &self.manifests.shared_locators
    }

    #[must_use]
    pub fn server_toolchain(&self, plan: &ServerPlan) -> Option<&ServerToolchain> {
        self.manifests
            .server(&plan.package)
            .and_then(|server| server.toolchain.as_ref())
    }
}
