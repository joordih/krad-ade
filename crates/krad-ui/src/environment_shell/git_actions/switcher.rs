use gpui_kit::Context;
use krad_repo::Branch;
use std::collections::HashSet;

use super::super::context_menu::{MenuAction, MenuCreate, MenuEntry, MenuSearch};
use super::super::groups::GroupGit;
use super::super::EnvironmentShell;
use super::GitAction;
use crate::components::IconName;

pub(in crate::environment_shell) const SEARCH_BRANCHES: &str =
    "Search branches or create a worktree…";
pub(in crate::environment_shell) const WORKTREES_HEADER: &str = "Worktrees";
const LOCAL_HEADER: &str = "Branches";
const REMOTE_HEADER: &str = "Remote branches";
const OPEN_AS_WORKTREE_LABEL: &str = "Open as worktree";

fn branch_entry(branch: &Branch) -> MenuEntry {
    let entry = MenuEntry::checked(
        branch.to_string(),
        branch.is_head,
        MenuAction::Git(GitAction::CheckOut(branch.clone())),
    )
    .with_options(vec![MenuEntry::item(
        OPEN_AS_WORKTREE_LABEL,
        Some(IconName::FolderAdd),
        MenuAction::Git(GitAction::OpenAsWorktree(branch.clone())),
    )]);
    match &branch.upstream {
        Some(upstream) => entry.with_trailing(upstream.clone()),
        None => entry,
    }
}

fn section(header: &str, entries: Vec<MenuEntry>) -> Vec<MenuEntry> {
    if entries.is_empty() {
        return entries;
    }
    std::iter::once(MenuEntry::Header(header.to_owned()))
        .chain(entries)
        .collect()
}

pub(in crate::environment_shell) fn branch_entries(git: &GroupGit) -> Vec<MenuEntry> {
    let in_worktrees: HashSet<&str> = git
        .worktrees
        .iter()
        .filter_map(|worktree| worktree.branch.as_deref())
        .collect();
    let tracked: HashSet<&str> = git
        .refs
        .local()
        .filter_map(|branch| branch.upstream.as_deref())
        .collect();
    let local = git
        .refs
        .local()
        .filter(|branch| !in_worktrees.contains(branch.name.as_str()))
        .map(branch_entry)
        .collect();
    let remote = git
        .refs
        .remote()
        .filter(|branch| !tracked.contains(branch.to_string().as_str()))
        .map(branch_entry)
        .collect();
    let mut entries = section(LOCAL_HEADER, local);
    entries.extend(section(REMOTE_HEADER, remote));
    entries
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_branch_switcher(
        &mut self,
        origin: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let group = self.active_group().map(|group| group.id);
        match self.branch_switcher_entries(group) {
            Ok((group, entries)) => {
                let search = MenuSearch {
                    placeholder: SEARCH_BRANCHES,
                    create: Some(MenuCreate::Worktree(group)),
                    footer: Vec::new(),
                };
                self.open_menu_searching(origin, entries, search, cx);
            }
            Err(note) => self.toast(note, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_repo::{BranchPlace, GitStatus, Refs, Worktree};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn branch(name: &str, place: BranchPlace, upstream: Option<&str>) -> Branch {
        Branch {
            name: name.to_owned(),
            place,
            upstream: upstream.map(str::to_owned),
            is_head: false,
        }
    }

    fn labels(entries: &[MenuEntry]) -> Vec<String> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Item { label, .. } => Some(label.clone()),
                MenuEntry::Header(header) => Some(format!("# {header}")),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn branches_in_a_worktree_and_remotes_a_local_branch_tracks_are_left_out() {
        let origin = || BranchPlace::Remote("origin".to_owned());
        let git = GroupGit {
            repo_root: PathBuf::from("/src/app"),
            status: GitStatus::default(),
            lines: HashMap::new(),
            worktrees: vec![Worktree {
                path: PathBuf::from("/src/app"),
                branch: Some("main".to_owned()),
            }],
            refs: Refs {
                branches: vec![
                    branch("main", BranchPlace::Local, Some("origin/main")),
                    branch("fix", BranchPlace::Local, None),
                    branch("main", origin(), None),
                    branch("topic", origin(), None),
                ],
                tags: Vec::new(),
            },
        };
        assert_eq!(
            labels(&branch_entries(&git)),
            ["# Branches", "fix", "# Remote branches", "origin/topic"]
        );
    }
}
