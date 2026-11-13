use gpui_kit::Context;
pub(in crate::environment_shell) use krad_domain::GroupGit;
use krad_domain::{ChangeKind, Host, LineCount};
use krad_protocol::{Command, GitAct, GitCommand, GitRead};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::super::language_servers::DiskChange;
use super::super::EnvironmentShell;

const GROUP_CHECKS_EVERY: Duration = Duration::from_secs(3);

pub(in crate::environment_shell) type GitKey = (Host, PathBuf);

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

impl EnvironmentShell {
    pub(in crate::environment_shell) fn start_group_checks(&mut self, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| loop {
            let checked = this.update(cx, |shell, cx| {
                let roots = shell.group_roots();
                shell.detect_wsl_agents(&roots);
                shell.ensure_project_checks(cx);
            });
            if checked.is_err() {
                break;
            }
            cx.background_executor().timer(GROUP_CHECKS_EVERY).await;
        });
        self.git_task = Some(task);
    }

    pub(in crate::environment_shell) fn refresh_git(&mut self, _: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(group) = self.envs.get(&env).and_then(|live| {
            live.selected_group
                .or_else(|| live.data.groups.first().map(|group| group.id))
        }) else {
            return;
        };
        self.runtime.client.post(Command::Git(GitCommand {
            environment: env,
            group,
            act: GitAct::Refresh,
        }));
    }

    pub(in crate::environment_shell) fn git_read_heard(
        &mut self,
        read: GitRead,
        cx: &mut Context<Self>,
    ) {
        let mut git = self.git.clone();
        let key = (read.host, read.root);
        match read.git {
            Some(read) => git.insert(key, *read),
            None => git.remove(&key),
        };
        self.apply_git(git, cx);
    }

    pub(in crate::environment_shell) fn git_replaced(
        &mut self,
        reads: Vec<GitRead>,
        cx: &mut Context<Self>,
    ) {
        let git = reads
            .into_iter()
            .filter_map(|read| Some(((read.host, read.root), *read.git?)))
            .collect();
        self.apply_git(git, cx);
    }

    fn apply_git(&mut self, git: HashMap<GitKey, GroupGit>, cx: &mut Context<Self>) {
        if self.git == git {
            return;
        }
        let dirs = changed_dirs(&self.git, &git);
        let files = changed_files(&self.git, &git);
        let vanished = any_vanished(&self.git, &git);
        self.git = git;
        let mut by_host: HashMap<Host, Vec<(PathBuf, DiskChange)>> = HashMap::new();
        for (host, path, change) in files {
            by_host.entry(host).or_default().push((path, change));
        }
        for (host, changed) in by_host {
            self.project_files_changed(&host, &changed, cx);
        }
        if vanished {
            self.forget_missing_problems(cx);
        }
        self.forget_diffs();
        self.forget_change_marks();
        self.forget_file_index();
        self.reload_file_dirs(&dirs, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{FileChange, GitStatus, Refs};

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
}
