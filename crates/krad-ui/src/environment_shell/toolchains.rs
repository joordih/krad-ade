mod activation;
mod entry;
mod label;
mod picker;
mod resolve;
mod status;

pub(super) use entry::ToolchainEntry;

use krad_domain::{
    EffectiveToolchain, Host, Toolchain, ToolchainChoices, ToolchainFamilyId, ToolchainScope,
};
use krad_extensions::{Discovery, ServerToolchain, ToolchainCache, ToolchainFamilyManifest};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::language_servers::Document;
use super::EnvironmentShell;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DocumentFamily {
    document: Document,
    family: ToolchainFamilyId,
}

#[derive(Default)]
pub(super) struct Toolchains {
    cache: Arc<ToolchainCache>,
    pub(super) choices: ToolchainChoices,
    scopes: HashMap<DocumentFamily, ToolchainScope>,
    discoveries: HashMap<ToolchainScope, Discovery>,
    resolving: HashSet<Document>,
    warned: HashSet<ToolchainScope>,
    entry: Option<ToolchainEntry>,
    pub(super) activations: activation::ActivationCache,
    statuses: HashMap<Document, Vec<status::StatusItem>>,
}

impl Toolchains {
    pub(super) fn with_choices(choices: ToolchainChoices) -> Self {
        Self {
            choices,
            ..Self::default()
        }
    }

    pub(super) fn scope_of(
        &self,
        document: &Document,
        family: &ToolchainFamilyId,
    ) -> Option<&ToolchainScope> {
        self.scopes.get(&DocumentFamily {
            document: document.clone(),
            family: family.clone(),
        })
    }

    pub(super) fn effective(&self, scope: &ToolchainScope) -> Option<EffectiveToolchain> {
        let discovery = self.discoveries.get(scope)?;
        Some(EffectiveToolchain::resolve(
            &self.choices.selection(scope),
            &discovery.candidates,
        ))
    }

    fn candidates(&self, scope: &ToolchainScope) -> &[Toolchain] {
        self.discoveries
            .get(scope)
            .map_or(&[][..], |discovery| discovery.candidates.as_slice())
    }

    pub(super) fn documents_in(&self, scope: &ToolchainScope) -> Vec<Document> {
        self.scopes
            .iter()
            .filter(|(_, known)| *known == scope)
            .map(|(key, _)| key.document.clone())
            .collect()
    }

    pub(super) fn served_toolchain(
        &self,
        host: &Host,
        folder: &Path,
        family: &ToolchainFamilyId,
    ) -> Option<Toolchain> {
        self.discoveries
            .keys()
            .find(|scope| {
                scope.host == *host && scope.family == *family && scope.folder() == folder
            })
            .and_then(|scope| self.effective(scope))
            .and_then(|effective| effective.toolchain().cloned())
    }

    pub(super) fn keep_documents(&mut self, open: &HashSet<Document>) {
        self.scopes.retain(|key, _| open.contains(&key.document));
        self.statuses.retain(|document, _| open.contains(document));
    }

    pub(super) fn forget_group(&mut self, host: &Host, group_root: &Path) {
        self.choices.forget_group(host, group_root);
        self.scopes
            .retain(|_, scope| !scope.belongs_to(host, group_root));
        self.discoveries
            .retain(|scope, _| !scope.belongs_to(host, group_root));
    }
}

impl EnvironmentShell {
    pub(super) fn language_families(&self, path: &Path) -> Vec<ToolchainFamilyManifest> {
        let Some(tools) = self.language_tools() else {
            return Vec::new();
        };
        self.language_of(path).map_or_else(Vec::new, |language| {
            tools
                .toolchain_families(&language)
                .into_iter()
                .cloned()
                .collect()
        })
    }

    pub(super) fn family_manifest(
        &self,
        id: &ToolchainFamilyId,
    ) -> Option<ToolchainFamilyManifest> {
        self.language_tools()?.toolchain_family(id).cloned()
    }

    pub(super) fn server_toolchain_of(
        &self,
        plan: &krad_extensions::ServerPlan,
    ) -> Option<ServerToolchain> {
        self.language_tools()?.server_toolchain(plan).cloned()
    }

    pub(super) fn group_root_for(&self, host: &Host, path: &Path) -> Option<PathBuf> {
        self.envs
            .values()
            .filter(|live| live.data.host == *host)
            .flat_map(|live| live.data.groups.iter())
            .filter(|group| path.starts_with(&group.root))
            .max_by_key(|group| group.root.components().count())
            .map(|group| group.root.clone())
            .or_else(|| path.parent().map(Path::to_path_buf))
    }

    pub(super) fn forget_group_toolchains(&mut self, host: &Host, group_root: &Path) {
        let still_open = self
            .envs
            .values()
            .filter(|live| live.data.host == *host)
            .any(|live| {
                live.data
                    .groups
                    .iter()
                    .any(|group| group.root == group_root)
            });
        if !still_open {
            self.toolchains.forget_group(host, group_root);
        }
    }

    pub(super) fn choose_toolchain(
        &mut self,
        scope: &ToolchainScope,
        toolchain: Toolchain,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        self.toolchains.warned.remove(scope);
        self.toolchains
            .choices
            .choose(scope.clone(), toolchain.clone());
        if let Some(discovery) = self.toolchains.discoveries.get_mut(scope) {
            if !discovery
                .candidates
                .iter()
                .any(|known| known.is_same(&toolchain))
            {
                discovery.candidates.push(toolchain);
            }
        }
        self.toolchain_changed(scope, cx);
    }

    pub(super) fn automatic_toolchain(
        &mut self,
        scope: &ToolchainScope,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        self.toolchains.warned.remove(scope);
        self.toolchains.choices.forget(scope);
        self.toolchain_changed(scope, cx);
    }

    pub(super) fn rescan_toolchains(
        &mut self,
        scope: &ToolchainScope,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        self.toolchains.cache.forget(scope);
        self.toolchains.activations.clear();
        for document in self.toolchains.documents_in(scope) {
            self.resolve_toolchains(document, cx);
        }
    }
}
