use gpui_kit::{AppContext as _, Context, Task};
use krad_domain::{Host, LanguageId};
use krad_extensions::{ProjectCheck, ServerScope};
use krad_host::HostLink;
use krad_lsp::lsp_types::FileChangeType;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::super::EnvironmentShell;
use super::{Document, ServerKey, ServerRoot};

mod disk;
mod opening;

const MAX_FILE_BYTES: u64 = 1024 * 1024;

pub(in crate::environment_shell) type GroupKey = (Host, PathBuf);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum DiskChange {
    Created,
    Changed,
    Deleted,
}

impl From<DiskChange> for FileChangeType {
    fn from(change: DiskChange) -> Self {
        match change {
            DiskChange::Created => Self::CREATED,
            DiskChange::Changed => Self::CHANGED,
            DiskChange::Deleted => Self::DELETED,
        }
    }
}

pub(in crate::environment_shell) struct Incomplete {
    pub(in crate::environment_shell) language: String,
    pub(in crate::environment_shell) checked: usize,
    pub(in crate::environment_shell) total: usize,
}

struct LanguageFiles {
    files: Vec<PathBuf>,
    total: usize,
}

enum GroupCheck {
    Indexing { _task: Task<()> },
    Indexed(BTreeMap<LanguageId, LanguageFiles>),
}

struct ServerCheck {
    group: GroupKey,
    language: LanguageId,
    opening: Option<Task<()>>,
}

#[derive(Default)]
pub(in crate::environment_shell) struct ProjectChecks {
    groups: HashMap<GroupKey, GroupCheck>,
    servers: HashMap<ServerKey, ServerCheck>,
    pub(in crate::environment_shell) background: HashSet<Document>,
    pub(in crate::environment_shell) representatives: HashSet<Document>,
    published: HashMap<ServerKey, HashSet<Document>>,
}

impl ProjectChecks {
    pub(in crate::environment_shell) fn wants(&self, key: &ServerKey) -> bool {
        self.servers.contains_key(key)
    }

    pub(in crate::environment_shell) fn published(&mut self, key: &ServerKey, document: &Document) {
        self.published
            .entry(key.clone())
            .or_default()
            .insert(document.clone());
    }

    fn files_of(&self, check: &ServerCheck) -> Option<&LanguageFiles> {
        match self.groups.get(&check.group)? {
            GroupCheck::Indexed(languages) => languages.get(&check.language),
            GroupCheck::Indexing { .. } => None,
        }
    }
}

pub(in crate::environment_shell) fn limited<T>(mut files: Vec<T>, limit: usize) -> (Vec<T>, usize) {
    let total = files.len();
    files.truncate(limit);
    (files, total)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn ensure_project_checks(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.envs.get(&self.active) else {
            return;
        };
        if !self.host_ready(&self.active) || self.language_servers.tools.is_none() {
            return;
        }
        let host = live.data.host.clone();
        let link = live.link.clone();
        let roots: Vec<PathBuf> = live
            .data
            .groups
            .iter()
            .map(|group| group.root.clone())
            .collect();
        for root in roots {
            let group = (host.clone(), root);
            match self.project_checks().groups.get(&group) {
                None => self.index_group(group, link.clone(), cx),
                Some(GroupCheck::Indexed(_)) => self.start_group_servers(&group, cx),
                Some(GroupCheck::Indexing { .. }) => {}
            }
        }
    }

    pub(in crate::environment_shell) const fn project_checks(&self) -> &ProjectChecks {
        &self.language_servers.project
    }

    fn index_group(&mut self, group: GroupKey, link: HostLink, cx: &mut Context<Self>) {
        let root = group.1.clone();
        let indexing = cx.background_spawn(async move { krad_repo::index_files(&link, &root) });
        let indexed = group.clone();
        let task = cx.spawn(async move |this, cx| {
            let index = indexing.await;
            this.update(cx, |shell, cx| {
                let Ok(index) = index else {
                    tracing::debug!("the project check could not list the group's files");
                    return;
                };
                let languages = shell.files_by_language(&indexed.1, index.files());
                shell
                    .language_servers
                    .project
                    .groups
                    .insert(indexed.clone(), GroupCheck::Indexed(languages));
                shell.start_group_servers(&indexed, cx);
            })
            .unwrap_or_default();
        });
        self.language_servers
            .project
            .groups
            .insert(group, GroupCheck::Indexing { _task: task });
    }

    fn files_by_language<'a>(
        &self,
        root: &Path,
        files: impl Iterator<Item = &'a Path>,
    ) -> BTreeMap<LanguageId, LanguageFiles> {
        let mut found: BTreeMap<LanguageId, Vec<PathBuf>> = BTreeMap::new();
        for file in files {
            let Some(language) = self.language_servers.catalog.for_path(file) else {
                continue;
            };
            found
                .entry(language.id.clone())
                .or_default()
                .push(root.join(file));
        }
        let limit =
            usize::try_from(self.settings.editor.project_check_limit.files()).unwrap_or(usize::MAX);
        found
            .into_iter()
            .map(|(language, files)| {
                let (files, total) = limited(files, limit);
                (language, LanguageFiles { files, total })
            })
            .collect()
    }

    fn start_group_servers(&mut self, group: &GroupKey, cx: &mut Context<Self>) {
        let Some(GroupCheck::Indexed(languages)) = self.project_checks().groups.get(group) else {
            return;
        };
        let firsts: Vec<(LanguageId, PathBuf)> = languages
            .iter()
            .filter_map(|(language, files)| Some((language.clone(), files.files.first()?.clone())))
            .collect();
        for (language, first) in firsts {
            let Some(key) = self.check_server_key(group, &first, cx) else {
                continue;
            };
            if self.project_checks().servers.contains_key(&key) {
                continue;
            }
            self.language_servers.project.servers.insert(
                key.clone(),
                ServerCheck {
                    group: group.clone(),
                    language,
                    opening: None,
                },
            );
            if self.language_servers.running(&key).is_some() {
                self.check_project(&key, cx);
            } else {
                self.ensure_server(key, cx);
            }
        }
    }

    fn check_server_key(
        &mut self,
        group: &GroupKey,
        first: &Path,
        cx: &mut Context<Self>,
    ) -> Option<ServerKey> {
        let tools = self.language_servers.tools.clone()?;
        let plan = tools.plan(self.language_servers.catalog.for_path(first)?)?;
        let document = Document::new(&group.0, first);
        if plan.scope == ServerScope::Workspace {
            let key = ServerKey {
                host: group.0.clone(),
                root: ServerRoot::Host,
                plan,
                toolchain: None,
            };
            self.register_folder(&key, group.1.clone());
            return Some(key);
        }
        if tools.server_toolchain(&plan).is_none() {
            return Some(ServerKey {
                host: group.0.clone(),
                root: ServerRoot::Folder(group.1.clone()),
                plan,
                toolchain: None,
            });
        }
        if self.toolchains_unresolved(&document) {
            self.language_servers
                .project
                .representatives
                .insert(document.clone());
            self.resolve_toolchains(document, cx);
            return None;
        }
        self.toolchain_server_key(&document, plan)
    }

    pub(in crate::environment_shell) fn check_project(
        &mut self,
        key: &ServerKey,
        cx: &mut Context<Self>,
    ) {
        match self.server_config(key).project_check {
            ProjectCheck::Pushed => {}
            ProjectCheck::WorkspacePull => self.pull_workspace(key, cx),
            ProjectCheck::OpenFiles => self.open_project_files(key, cx),
        }
    }

    pub(in crate::environment_shell) fn incomplete_checks(&self, host: &Host) -> Vec<Incomplete> {
        let checks = self.project_checks();
        let mut shown: Vec<Incomplete> = checks
            .servers
            .iter()
            .filter(|(key, _)| key.host == *host)
            .filter_map(|(_, check)| {
                let files = checks.files_of(check)?;
                (files.total > files.files.len()).then(|| Incomplete {
                    language: self
                        .language_servers
                        .catalog
                        .get(&check.language)
                        .map_or_else(
                            || check.language.to_string(),
                            |language| language.name.clone(),
                        ),
                    checked: files.files.len(),
                    total: files.total,
                })
            })
            .collect();
        shown.sort_by(|left, right| left.language.cmp(&right.language));
        shown.dedup_by(|left, right| left.language == right.language);
        shown
    }

    pub(in crate::environment_shell) fn forget_server_check(&mut self, key: &ServerKey) {
        self.language_servers.project.servers.remove(key);
        let published = self
            .language_servers
            .project
            .published
            .remove(key)
            .unwrap_or_default();
        for document in published {
            self.language_servers.diagnostics.forget(&document);
        }
        let orphans: Vec<Document> = self
            .language_servers
            .project
            .background
            .iter()
            .filter(|document| {
                self.language_servers
                    .documents
                    .get(*document)
                    .is_none_or(|served| served == key)
            })
            .cloned()
            .collect();
        for document in orphans {
            self.language_servers.project.background.remove(&document);
            self.language_servers.diagnostics.forget(&document);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_keeps_the_limit_of_files_and_counts_them_all() {
        let (kept, total) = limited(vec![1, 2, 3, 4, 5], 3);
        assert_eq!(kept, [1, 2, 3]);
        assert_eq!(total, 5);
        let (kept, total) = limited(vec![1], 3);
        assert_eq!((kept.len(), total), (1, 1));
    }
}
