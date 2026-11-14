use krad_domain::{ChangeKind, GroupGit, Host, LineCount};
use krad_protocol::GitRead;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::super::language_servers::DiskChange;

pub(in crate::environment_shell) type GitKey = (Host, PathBuf);

#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) struct GitGroups(HashMap<GitKey, GroupGit>);

#[derive(Default)]
pub(in crate::environment_shell) struct GitMirror {
    groups: Rc<GitGroups>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) struct GitMove {
    pub(in crate::environment_shell) dirs: HashSet<PathBuf>,
    pub(in crate::environment_shell) files: Vec<(Host, PathBuf, DiskChange)>,
    pub(in crate::environment_shell) vanished: bool,
}

impl GitMirror {
    pub(in crate::environment_shell) fn current(&self) -> Rc<GitGroups> {
        Rc::clone(&self.groups)
    }

    pub(in crate::environment_shell) fn read(&mut self, read: GitRead) -> Option<GitMove> {
        let mut groups = self.groups.0.clone();
        let key = (read.host, read.root);
        match read.git {
            Some(read) => groups.insert(key, *read),
            None => groups.remove(&key),
        };
        self.adopt(groups)
    }

    pub(in crate::environment_shell) fn replace(&mut self, reads: Vec<GitRead>) -> Option<GitMove> {
        let groups = reads
            .into_iter()
            .filter_map(|read| Some(((read.host, read.root), *read.git?)))
            .collect();
        self.adopt(groups)
    }

    fn adopt(&mut self, groups: HashMap<GitKey, GroupGit>) -> Option<GitMove> {
        let before = &self.groups.0;
        if *before == groups {
            return None;
        }
        let moved = GitMove {
            dirs: changed_dirs(before, &groups),
            files: changed_files(before, &groups),
            vanished: any_vanished(before, &groups),
        };
        self.groups = Rc::new(GitGroups(groups));
        Some(moved)
    }
}

impl GitGroups {
    pub(in crate::environment_shell) fn at(&self, host: &Host, root: &Path) -> Option<&GroupGit> {
        self.0.get(&(host.clone(), root.to_path_buf()))
    }

    pub(in crate::environment_shell) fn on<'a>(
        &'a self,
        host: &'a Host,
    ) -> impl Iterator<Item = &'a GroupGit> + 'a {
        self.0
            .iter()
            .filter(move |((owner, _), _)| owner == host)
            .map(|(_, git)| git)
    }
}

fn changed_dirs(
    before: &HashMap<GitKey, GroupGit>,
    after: &HashMap<GitKey, GroupGit>,
) -> HashSet<PathBuf> {
    let changes = |git: &HashMap<GitKey, GroupGit>| -> HashSet<(PathBuf, ChangeKind)> {
        git.values()
            .flat_map(|group| {
                group
                    .status
                    .changes
                    .iter()
                    .map(|change| (group.repo_root.join(&change.path), change.kind))
            })
            .collect()
    };
    changes(before)
        .symmetric_difference(&changes(after))
        .filter_map(|(path, _)| path.parent().map(Path::to_path_buf))
        .collect()
}

type FileState = (ChangeKind, Option<LineCount>);

fn file_states(git: &HashMap<GitKey, GroupGit>) -> HashMap<(Host, PathBuf), FileState> {
    git.iter()
        .flat_map(|((host, _), group)| {
            group.status.changes.iter().map(move |change| {
                (
                    (host.clone(), group.repo_root.join(&change.path)),
                    (change.kind, group.lines.get(&change.path).copied()),
                )
            })
        })
        .collect()
}

const fn disk_change(before: Option<FileState>, after: Option<FileState>) -> DiskChange {
    match (before, after) {
        (_, Some((ChangeKind::Deleted, _))) => DiskChange::Deleted,
        (None, Some((ChangeKind::Untracked | ChangeKind::Added, _))) => DiskChange::Created,
        _ => DiskChange::Changed,
    }
}

const fn may_have_vanished(before: Option<FileState>, after: Option<FileState>) -> bool {
    matches!(
        (before, after),
        (Some((ChangeKind::Untracked | ChangeKind::Added, _)), None)
    )
}

fn any_vanished(before: &HashMap<GitKey, GroupGit>, after: &HashMap<GitKey, GroupGit>) -> bool {
    let after = file_states(after);
    file_states(before)
        .into_iter()
        .any(|(place, was)| may_have_vanished(Some(was), after.get(&place).copied()))
}

fn changed_files(
    before: &HashMap<GitKey, GroupGit>,
    after: &HashMap<GitKey, GroupGit>,
) -> Vec<(Host, PathBuf, DiskChange)> {
    let (before, after) = (file_states(before), file_states(after));
    let places: HashSet<&(Host, PathBuf)> = before.keys().chain(after.keys()).collect();
    let mut changed: Vec<(Host, PathBuf, DiskChange)> = places
        .into_iter()
        .filter_map(|place| {
            let (was, now) = (before.get(place).copied(), after.get(place).copied());
            (was != now).then(|| (place.0.clone(), place.1.clone(), disk_change(was, now)))
        })
        .collect();
    changed.sort_by(|left, right| left.1.cmp(&right.1));
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{FileChange, GitStatus, Refs};
    use krad_protocol::GitRead;

    fn git(changes: &[&str]) -> HashMap<GitKey, GroupGit> {
        HashMap::from([(
            (Host::Local, PathBuf::from("/r")),
            GroupGit {
                repo_root: PathBuf::from("/r"),
                status: GitStatus {
                    changes: changes
                        .iter()
                        .map(|path| FileChange {
                            path: PathBuf::from(path),
                            kind: ChangeKind::Modified,
                            origin: None,
                        })
                        .collect(),
                    ..GitStatus::default()
                },
                lines: HashMap::new(),
                worktrees: Vec::new(),
                refs: Refs::default(),
            },
        )])
    }

    #[test]
    fn files_whose_change_or_line_count_moved_are_told_to_the_language_servers() {
        let before = git(&["src/a.rs", "README.md"]);
        let mut after = git(&["README.md", "src/new.rs"]);
        if let Some(group) = after.values_mut().next() {
            group.status.changes[1].kind = ChangeKind::Untracked;
            group.lines.insert(
                PathBuf::from("README.md"),
                LineCount {
                    added: 3,
                    removed: 0,
                },
            );
        }
        let changed: Vec<(PathBuf, DiskChange)> = changed_files(&before, &after)
            .into_iter()
            .map(|(_, path, change)| (path, change))
            .collect();
        assert_eq!(
            changed,
            [
                (PathBuf::from("/r/README.md"), DiskChange::Changed),
                (PathBuf::from("/r/src/a.rs"), DiskChange::Changed),
                (PathBuf::from("/r/src/new.rs"), DiskChange::Created),
            ]
        );
        assert_eq!(changed_files(&before, &before), Vec::new());
    }

    #[test]
    fn an_untracked_file_leaving_the_status_may_have_been_deleted() {
        let mut before = git(&["src/new.rs", "README.md"]);
        if let Some(group) = before.values_mut().next() {
            group.status.changes[0].kind = ChangeKind::Untracked;
        }
        assert!(any_vanished(&before, &git(&["README.md"])));
        assert!(!any_vanished(&git(&["src/a.rs"]), &git(&[])));
    }

    #[test]
    fn only_folders_holding_a_changed_file_reload() {
        let dirs = changed_dirs(
            &git(&["src/a.rs", "README.md"]),
            &git(&["README.md", "docs/b.md"]),
        );
        assert_eq!(
            dirs,
            HashSet::from([PathBuf::from("/r/src"), PathBuf::from("/r/docs")])
        );
    }

    #[test]
    fn the_mirror_reports_what_a_read_moved_and_shares_its_groups() {
        let mut mirror = GitMirror::default();
        let read = |changes: &[&str]| GitRead {
            host: Host::Local,
            root: PathBuf::from("/r"),
            git: git(changes).into_values().next().map(Box::new),
        };
        let moved = mirror.read(read(&["src/a.rs"]));
        assert_eq!(
            moved.map(|moved| moved.dirs),
            Some(HashSet::from([PathBuf::from("/r/src")]))
        );
        assert_eq!(mirror.read(read(&["src/a.rs"])), None);
        let shared = mirror.current();
        assert!(shared.at(&Host::Local, Path::new("/r")).is_some());
        assert_eq!(shared.on(&Host::Local).count(), 1);
        assert!(mirror
            .replace(Vec::new())
            .is_some_and(|moved| moved.dirs.len() == 1));
        assert!(mirror.current().at(&Host::Local, Path::new("/r")).is_none());
    }
}
