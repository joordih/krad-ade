use krad_domain::{
    rank_toolchains, ProjectFolder, ProjectHints, RankContext, Toolchain, ToolchainFamilyId,
    ToolchainScope,
};
use krad_host::HostLink;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use super::hints::{merge, read_hint};
use super::locate::{locate, locate_entered, LocateContext};
use super::manifest::{ProjectConfig, SharedLocatorManifest, ToolchainFamilyManifest};
use crate::error::ExtensionError;

const HINT_FILE_LIMIT: u64 = 256 * 1024;
const CACHE_LIFETIME: Duration = Duration::from_mins(10);
const TABLE_SEPARATOR: char = '.';

#[derive(Clone, Copy, Debug)]
pub struct DiscoveryRequest<'a> {
    pub link: &'a HostLink,
    pub group_root: &'a Path,
    pub project_folder: &'a ProjectFolder,
    pub home: Option<&'a Path>,
    pub host_cli: Option<&'a Path>,
    pub timeout: Duration,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Discovery {
    pub candidates: Vec<Toolchain>,
    pub hints: ProjectHints,
}

impl Discovery {
    #[must_use]
    pub fn best(&self) -> Option<&Toolchain> {
        self.candidates.first()
    }
}

fn folders(request: &DiscoveryRequest<'_>) -> Vec<PathBuf> {
    let project = request.project_folder.inside(request.group_root);
    if project == request.group_root {
        vec![project]
    } else {
        vec![project, request.group_root.to_path_buf()]
    }
}

fn read_hints(family: &ToolchainFamilyManifest, request: &DiscoveryRequest<'_>) -> ProjectHints {
    let mut found = Vec::new();
    for folder in folders(request) {
        for spec in &family.hints {
            let file = folder.join(&spec.file);
            let Ok(bytes) = request.link.read(&file, HINT_FILE_LIMIT) else {
                continue;
            };
            let text = String::from_utf8_lossy(&bytes);
            found.extend(read_hint(spec, &text, &file, &folder));
        }
    }
    merge(found)
}

#[must_use]
pub fn discover(
    family: &ToolchainFamilyManifest,
    shared: &[SharedLocatorManifest],
    request: &DiscoveryRequest<'_>,
) -> Discovery {
    let project = request.project_folder.inside(request.group_root);
    let context = LocateContext {
        link: request.link,
        family: &family.id,
        home: request.home,
        group_root: request.group_root,
        project_folder: &project,
        timeout: request.timeout,
        host_cli: request.host_cli,
        shared,
        entered: None,
    };
    let found: Vec<Toolchain> = std::thread::scope(|scope| {
        let runs: Vec<_> = family
            .locators
            .iter()
            .map(|spec| {
                let context = &context;
                scope.spawn(move || locate(context, spec))
            })
            .collect();
        runs.into_iter()
            .filter_map(|run| run.join().ok())
            .flatten()
            .collect()
    });
    let hints = read_hints(family, request);
    let candidates = rank_toolchains(
        found,
        &RankContext {
            group_root: request.group_root,
            project_folder: &project,
            priority: &family.priority,
            hints: &hints,
        },
    );
    tracing::debug!(
        family = %family.id,
        candidates = candidates.len(),
        "toolchains discovered"
    );
    Discovery { candidates, hints }
}

#[must_use]
pub fn entered_toolchain(
    family: &ToolchainFamilyManifest,
    shared: &[SharedLocatorManifest],
    request: &DiscoveryRequest<'_>,
    entered: &Path,
) -> Option<Toolchain> {
    let project = request.project_folder.inside(request.group_root);
    let context = LocateContext {
        link: request.link,
        family: &family.id,
        home: request.home,
        group_root: request.group_root,
        project_folder: &project,
        timeout: request.timeout,
        host_cli: request.host_cli,
        shared,
        entered: Some(entered),
    };
    locate_entered(&context, &family.entered)
}

/// # Errors
pub fn project_folder_for(
    link: &HostLink,
    family: &ToolchainFamilyManifest,
    group_root: &Path,
    file: &Path,
) -> Result<ProjectFolder, ExtensionError> {
    let folder = file.parent().unwrap_or(group_root);
    let relative = ProjectFolder::of(folder, group_root)
        .map_err(|error| ExtensionError::Template(error.to_string()))?;
    let marked: BTreeSet<ProjectFolder> = relative
        .as_path()
        .ancestors()
        .filter_map(|ancestor| ProjectFolder::try_from(ancestor.to_path_buf()).ok())
        .filter(|candidate| {
            let inside = candidate.inside(group_root);
            family
                .markers
                .iter()
                .any(|marker| link.exists(&inside.join(marker)))
        })
        .collect();
    Ok(ProjectFolder::nearest(&relative, &marked))
}

#[must_use]
pub fn project_config_present(link: &HostLink, folder: &Path, config: &ProjectConfig) -> bool {
    let file = folder.join(&config.file);
    let Some(table) = &config.table else {
        return link.exists(&file);
    };
    let Ok(bytes) = link.read(&file, HINT_FILE_LIMIT) else {
        return false;
    };
    let Ok(document) = String::from_utf8_lossy(&bytes).parse::<toml::Table>() else {
        return false;
    };
    let mut current = Some(&document);
    for key in table.split(TABLE_SEPARATOR) {
        current = current
            .and_then(|level| level.get(key))
            .and_then(toml::Value::as_table);
    }
    current.is_some()
}

struct Cached {
    at: Instant,
    stamps: Vec<Option<SystemTime>>,
    discovery: Discovery,
}

#[derive(Default)]
pub struct ToolchainCache {
    entries: Mutex<HashMap<ToolchainScope, Cached>>,
}

fn stamps(
    family: &ToolchainFamilyManifest,
    request: &DiscoveryRequest<'_>,
) -> Vec<Option<SystemTime>> {
    folders(request)
        .iter()
        .flat_map(|folder| {
            family
                .markers
                .iter()
                .chain(family.hints.iter().map(|hint| &hint.file))
                .map(move |file| folder.join(file))
        })
        .map(|file| {
            request
                .link
                .metadata(&file)
                .ok()
                .and_then(|metadata| metadata.modified)
        })
        .collect()
}

impl ToolchainCache {
    #[must_use]
    pub fn discover(
        &self,
        scope: &ToolchainScope,
        family: &ToolchainFamilyManifest,
        shared: &[SharedLocatorManifest],
        request: &DiscoveryRequest<'_>,
    ) -> Discovery {
        let current = stamps(family, request);
        {
            let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(cached) = entries.get(scope) {
                if cached.at.elapsed() < CACHE_LIFETIME && cached.stamps == current {
                    return cached.discovery.clone();
                }
            }
        }
        let discovery = discover(family, shared, request);
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                scope.clone(),
                Cached {
                    at: Instant::now(),
                    stamps: current,
                    discovery: discovery.clone(),
                },
            );
        discovery
    }

    pub fn forget(&self, scope: &ToolchainScope) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(scope);
    }

    pub fn forget_family(&self, family: &ToolchainFamilyId) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|scope, _| scope.family != *family);
    }
}
