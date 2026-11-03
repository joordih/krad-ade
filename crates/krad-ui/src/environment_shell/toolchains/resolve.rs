use gpui_kit::{AppContext as _, Context};
use krad_domain::{
    EffectiveToolchain, Host, ProjectFolder, ToolchainChoices, ToolchainFamilyId, ToolchainScope,
    ToolchainSelection,
};
use krad_extensions::{
    project_folder_for, Discovery, DiscoveryRequest, SharedLocatorManifest, ToolchainCache,
    ToolchainFamilyManifest,
};
use krad_host::{HostLink, HOST_CLI_PROGRAM};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::super::language_servers::Document;
use super::super::EnvironmentShell;
use super::DocumentFamily;

pub(super) struct DiscoveryInputs {
    pub(super) link: HostLink,
    pub(super) host: Host,
    pub(super) group_root: PathBuf,
    pub(super) home: Option<PathBuf>,
    pub(super) host_cli: Option<PathBuf>,
    pub(super) timeout: Duration,
}

impl DiscoveryInputs {
    pub(super) fn request<'a>(&'a self, folder: &'a ProjectFolder) -> DiscoveryRequest<'a> {
        DiscoveryRequest {
            link: &self.link,
            group_root: &self.group_root,
            project_folder: folder,
            home: self.home.as_deref(),
            host_cli: self.host_cli.as_deref(),
            timeout: self.timeout,
        }
    }

    pub(super) fn scope(
        &self,
        folder: ProjectFolder,
        family: &ToolchainFamilyId,
    ) -> ToolchainScope {
        ToolchainScope {
            host: self.host.clone(),
            group_root: self.group_root.clone(),
            project_folder: folder,
            family: family.clone(),
        }
    }
}

pub(super) struct Resolved {
    family: ToolchainFamilyId,
    scope: ToolchainScope,
    discovery: Discovery,
}

pub(super) fn discover_scope(
    inputs: &DiscoveryInputs,
    cache: &ToolchainCache,
    family: &ToolchainFamilyManifest,
    shared: &[SharedLocatorManifest],
    choices: &ToolchainChoices,
    folder: &ProjectFolder,
) -> (ToolchainScope, Discovery) {
    let scope = inputs.scope(folder.clone(), &family.id);
    let mut discovery = cache.discover(&scope, family, shared, &inputs.request(folder));
    if let ToolchainSelection::Chosen(chosen) = choices.selection(&scope) {
        let listed = discovery
            .candidates
            .iter()
            .any(|candidate| candidate.is_same(&chosen));
        let present = chosen.path().is_some_and(|path| inputs.link.exists(path));
        if !listed && present {
            discovery.candidates.push(*chosen);
        }
    }
    (scope, discovery)
}

fn resolve_all(
    inputs: &DiscoveryInputs,
    cache: &ToolchainCache,
    families: &[ToolchainFamilyManifest],
    shared: &[SharedLocatorManifest],
    choices: &ToolchainChoices,
    file: &Path,
) -> Vec<Resolved> {
    families
        .iter()
        .map(|family| {
            let folder = project_folder_for(&inputs.link, family, &inputs.group_root, file)
                .unwrap_or_default();
            let (scope, discovery) =
                discover_scope(inputs, cache, family, shared, choices, &folder);
            Resolved {
                family: family.id.clone(),
                scope,
                discovery,
            }
        })
        .collect()
}

impl EnvironmentShell {
    pub(super) fn discovery_inputs(
        &self,
        host: &Host,
        group_root: PathBuf,
    ) -> Option<DiscoveryInputs> {
        let (name, live) = self.envs.iter().find(|(_, live)| live.data.host == *host)?;
        let link = live.link.clone();
        let host_cli = match &link {
            HostLink::Local => self.host_tools.local_cli.clone(),
            HostLink::Ssh(ssh) => ssh
                .layout()
                .map(|layout| layout.bin_dir().join(HOST_CLI_PROGRAM)),
        };
        Some(DiscoveryInputs {
            home: self.env_home(name),
            link,
            host: host.clone(),
            group_root,
            host_cli,
            timeout: self.settings.host_timeouts.command.duration(),
        })
    }

    pub(in crate::environment_shell) fn toolchains_unresolved(&self, document: &Document) -> bool {
        !self.toolchains.resolving.contains(document)
            && self
                .language_families(&document.path)
                .iter()
                .any(|family| self.toolchains.scope_of(document, &family.id).is_none())
    }

    pub(in crate::environment_shell) fn toolchain_resolving(&self, document: &Document) -> bool {
        self.toolchains.resolving.contains(document)
    }

    pub(in crate::environment_shell) fn resolve_toolchains(
        &mut self,
        document: Document,
        cx: &mut Context<Self>,
    ) {
        if self.toolchains.resolving.contains(&document) || !self.host_is_ready(&document.host) {
            return;
        }
        let families = self.language_families(&document.path);
        let Some(tools) = self.language_tools() else {
            return;
        };
        let Some(root) = self.group_root_for(&document.host, &document.path) else {
            return;
        };
        let Some(inputs) = self.discovery_inputs(&document.host, root) else {
            return;
        };
        if families.is_empty() {
            return;
        }
        self.toolchains.resolving.insert(document.clone());
        self.refresh_toolchain_status(&document);
        let shared: Vec<SharedLocatorManifest> = tools.shared_locators().to_vec();
        let cache = Arc::clone(&self.toolchains.cache);
        let choices = self.toolchains.choices.clone();
        let file = document.path.clone();
        let resolving = cx.background_spawn(async move {
            resolve_all(&inputs, &cache, &families, &shared, &choices, &file)
        });
        cx.spawn(async move |this, cx| {
            let resolved = resolving.await;
            this.update(cx, |shell, cx| {
                shell.toolchains_resolved(&document, resolved, cx);
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    fn host_is_ready(&self, host: &Host) -> bool {
        self.envs
            .iter()
            .find(|(_, live)| live.data.host == *host)
            .is_some_and(|(name, _)| self.host_ready(name))
    }

    fn toolchains_resolved(
        &mut self,
        document: &Document,
        resolved: Vec<Resolved>,
        cx: &mut Context<Self>,
    ) {
        self.toolchains.resolving.remove(document);
        for found in resolved {
            self.warn_if_choice_vanished(&found.scope, &found.discovery);
            self.toolchains.scopes.insert(
                DocumentFamily {
                    document: document.clone(),
                    family: found.family,
                },
                found.scope.clone(),
            );
            let changed = self
                .toolchains
                .discoveries
                .insert(found.scope.clone(), found.discovery.clone())
                .is_some_and(|previous| previous != found.discovery);
            if changed {
                self.toolchain_changed(&found.scope, cx);
            }
        }
        self.refresh_toolchain_status(document);
        self.reattach_document(document, cx);
        cx.notify();
    }

    fn warn_if_choice_vanished(&mut self, scope: &ToolchainScope, discovery: &Discovery) {
        let effective = EffectiveToolchain::resolve(
            &self.toolchains.choices.selection(scope),
            &discovery.candidates,
        );
        let EffectiveToolchain::ChosenMissing { chosen, fallback } = effective else {
            return;
        };
        if !self.toolchains.warned.insert(scope.clone()) {
            return;
        }
        tracing::warn!(family = %scope.family, "a chosen toolchain is no longer found");
        let using = fallback.map_or_else(
            || super::label::NO_FALLBACK_TEXT.to_owned(),
            |fallback| format!("{} {}", super::label::USING_TEXT, fallback.name),
        );
        self.push_toast(format!(
            "{} {} {}",
            chosen.name,
            super::label::VANISHED_TEXT,
            using
        ));
    }
}
