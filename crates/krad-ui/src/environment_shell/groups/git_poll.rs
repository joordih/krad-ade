use gpui_kit::{AppContext as _, Context};
use krad_domain::Host;
use krad_host::HostLink;
use krad_repo::{
    line_counts, list_refs, list_worktrees, repo_root, status, ChangeKind, GitStatus, LineCount,
    Refs, Worktree,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::super::language_servers::DiskChange;
use super::super::EnvironmentShell;

const LOCAL_POLL: Duration = Duration::from_secs(3);
const REMOTE_POLL: Duration = Duration::from_secs(5);
const SLOWEST_POLL: Duration = Duration::from_secs(20);
const QUIET_GROWTH: u32 = 2;

pub(in crate::environment_shell) type GitKey = (Host, PathBuf);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct GroupGit {
    pub(in crate::environment_shell) repo_root: PathBuf,
    pub(in crate::environment_shell) status: GitStatus,
    pub(in crate::environment_shell) lines: HashMap<PathBuf, LineCount>,
    pub(in crate::environment_shell) worktrees: Vec<Worktree>,
    pub(in crate::environment_shell) refs: Refs,
}

impl GroupGit {
    pub(in crate::environment_shell) fn total_lines(&self) -> LineCount {
        self.lines
            .values()
            .copied()
            .fold(LineCount::default(), |total, count| total + count)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PollPace {
    fastest: Duration,
    current: Duration,
}

impl PollPace {
    const fn new(remote: bool) -> Self {
        let fastest = if remote { REMOTE_POLL } else { LOCAL_POLL };
        Self {
            fastest,
            current: fastest,
        }
    }

    fn after(self, changed: bool, remote: bool) -> Self {
        let start = Self::new(remote);
        if changed || start.fastest != self.fastest {
            return start;
        }
        Self {
            current: self.current.saturating_mul(QUIET_GROWTH).min(SLOWEST_POLL),
            ..self
        }
    }
}

fn read_root(link: &HostLink, root: &Path) -> Option<GroupGit> {
    let repo_root = repo_root(link, root).ok().flatten()?;
    std::thread::scope(|scope| {
        let lines = scope.spawn(|| line_counts(link, &repo_root).unwrap_or_default());
        let worktrees = scope.spawn(|| list_worktrees(link, root).unwrap_or_default());
        let refs = scope.spawn(|| list_refs(link, root).unwrap_or_default());
        let status = status(link, root).ok();
        let lines = lines.join().unwrap_or_default();
        let worktrees = worktrees.join().unwrap_or_default();
        let refs = refs.join().unwrap_or_default();
        Some(GroupGit {
            status: status?,
            repo_root: repo_root.clone(),
            lines,
            worktrees,
            refs,
        })
    })
}

fn read_git(link: &HostLink, roots: Vec<PathBuf>) -> HashMap<GitKey, GroupGit> {
    if !link.is_ready() {
        return HashMap::new();
    }
    let host = link.host();
    std::thread::scope(|scope| {
        let reads: Vec<_> = roots
            .into_iter()
            .map(|root| scope.spawn(move || Some((root.clone(), read_root(link, &root)?))))
            .collect();
        reads
            .into_iter()
            .filter_map(|read| read.join().ok().flatten())
            .map(|(root, git)| ((host.clone(), root), git))
            .collect()
    })
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

impl EnvironmentShell {
    pub(in crate::environment_shell) fn start_git_watch(&mut self, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| {
            let mut pace = PollPace::new(false);
            loop {
                let Ok((link, roots)) = this.update(cx, |shell, cx| {
                    let (link, roots) = shell.group_roots();
                    shell.detect_wsl_agents(&roots, cx);
                    shell.ensure_project_checks(cx);
                    (link, roots)
                }) else {
                    break;
                };
                let remote = !link.is_local();
                let git = cx
                    .background_executor()
                    .spawn(async move { read_git(&link, roots) })
                    .await;
                let Ok(changed) = this.update(cx, |shell, cx| {
                    let changed = shell.apply_git(git, cx);
                    shell.sweep_merged_worktrees(cx);
                    changed
                }) else {
                    break;
                };
                pace = pace.after(changed, remote);
                cx.background_executor().timer(pace.current).await;
            }
        });
        self.git_task = Some(task);
    }

    pub(in crate::environment_shell) fn refresh_git(&mut self, cx: &mut Context<Self>) {
        let (link, roots) = self.group_roots();
        let read = cx.background_spawn(async move { read_git(&link, roots) });
        cx.spawn(async move |this, cx| {
            let git = read.await;
            this.update(cx, |shell, cx| shell.apply_git(git, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn apply_git(&mut self, git: HashMap<GitKey, GroupGit>, cx: &mut Context<Self>) -> bool {
        if self.git == git {
            return false;
        }
        let dirs = changed_dirs(&self.git, &git);
        let files = changed_files(&self.git, &git);
        let vanished = any_vanished(&self.git, &git);
        let changed_roots = super::super::app_events::changed_git_roots(&self.git, &git);
        self.git = git;
        self.announce_git_changes(&changed_roots);
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
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_repo::FileChange;

    #[test]
    fn a_quiet_repository_is_polled_less_often_until_it_changes() {
        let start = PollPace::new(true);
        assert_eq!(start.current, REMOTE_POLL);
        let quiet = start.after(false, true).after(false, true);
        assert_eq!(quiet.current, REMOTE_POLL * 4);
        let slowest = (0..10).fold(quiet, |pace, _| pace.after(false, true));
        assert_eq!(slowest.current, SLOWEST_POLL);
        assert_eq!(slowest.after(true, true).current, REMOTE_POLL);
        assert_eq!(slowest.after(false, false).current, LOCAL_POLL);
    }

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
