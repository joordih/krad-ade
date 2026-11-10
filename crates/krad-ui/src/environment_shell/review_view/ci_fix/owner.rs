use krad_domain::{GroupId, TerminalId};
use krad_repo::{Branch, BranchPlace, ORIGIN_REMOTE};
use std::path::Path;

use crate::environment_shell::groups::GroupGit;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct ChatSeat {
    pub(super) id: TerminalId,
    pub(super) group: Option<GroupId>,
    pub(super) worker: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum FixOwner {
    Chat(TerminalId),
    Group(GroupId),
    Nobody,
}

pub(super) fn branch_owner<'a>(
    groups: impl IntoIterator<Item = (GroupId, Option<&'a str>)>,
    chats: impl IntoIterator<Item = ChatSeat>,
    branch: &str,
) -> FixOwner {
    let on_branch: Vec<GroupId> = groups
        .into_iter()
        .filter(|(_, checked_out)| *checked_out == Some(branch))
        .map(|(group, _)| group)
        .collect();
    let Some(first) = on_branch.first().copied() else {
        return FixOwner::Nobody;
    };
    let seated: Vec<ChatSeat> = chats
        .into_iter()
        .filter(|chat| chat.group.is_some_and(|group| on_branch.contains(&group)))
        .collect();
    seated
        .iter()
        .find(|chat| !chat.worker)
        .or_else(|| seated.first())
        .map_or(FixOwner::Group(first), |chat| FixOwner::Chat(chat.id))
}

pub(super) fn shares_repository(repository: &GroupGit, root: &Path) -> bool {
    in_worktrees(
        &repository.repo_root,
        repository
            .worktrees
            .iter()
            .map(|worktree| worktree.path.as_path()),
        root,
    )
}

fn in_worktrees<'a>(
    repo_root: &Path,
    mut worktrees: impl Iterator<Item = &'a Path>,
    root: &Path,
) -> bool {
    root.starts_with(repo_root) || worktrees.any(|worktree| root.starts_with(worktree))
}

pub(super) fn pick_branch<'a>(branches: &'a [Branch], name: &str) -> Option<&'a Branch> {
    let named = || branches.iter().filter(move |branch| branch.name == name);
    named()
        .find(|branch| branch.place == BranchPlace::Local)
        .or_else(|| {
            named().find(|branch| branch.place == BranchPlace::Remote(ORIGIN_REMOTE.to_owned()))
        })
        .or_else(|| named().next())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRANCH: &str = "fix-durations";

    fn seat(id: u64, group: u64, worker: bool) -> ChatSeat {
        ChatSeat {
            id: TerminalId(id),
            group: Some(GroupId(group)),
            worker,
        }
    }

    fn branch(name: &str, place: BranchPlace) -> Branch {
        Branch {
            name: name.to_owned(),
            place,
            upstream: None,
            is_head: false,
        }
    }

    #[test]
    fn the_branchs_lead_chat_gets_the_fix_before_its_workers() {
        let groups = [(GroupId(1), Some("main")), (GroupId(2), Some(BRANCH))];
        let chats = [seat(10, 1, false), seat(11, 2, true), seat(12, 2, false)];
        assert_eq!(
            branch_owner(groups, chats, BRANCH),
            FixOwner::Chat(TerminalId(12))
        );
        let only_worker = [seat(11, 2, true)];
        assert_eq!(
            branch_owner(groups, only_worker, BRANCH),
            FixOwner::Chat(TerminalId(11))
        );
    }

    #[test]
    fn a_branch_without_chats_names_its_group_and_one_nobody_has_names_nobody() {
        let groups = [(GroupId(1), Some("main")), (GroupId(2), Some(BRANCH))];
        assert_eq!(
            branch_owner(groups, [seat(10, 1, false)], BRANCH),
            FixOwner::Group(GroupId(2))
        );
        assert_eq!(
            branch_owner([(GroupId(1), None)], [seat(10, 1, false)], BRANCH),
            FixOwner::Nobody
        );
    }

    #[test]
    fn only_groups_inside_the_repository_or_its_worktrees_can_own_the_fix() {
        let repo = Path::new("/code/app");
        let worktrees = [Path::new("/code/app"), Path::new("/code/app.worktrees/fix")];
        let inside = |root: &str| in_worktrees(repo, worktrees.iter().copied(), Path::new(root));
        assert!(inside("/code/app"));
        assert!(inside("/code/app/crates/core"));
        assert!(inside("/code/app.worktrees/fix"));
        assert!(!inside("/code/other"));
        assert!(!inside("/code/app-fork"));
    }

    #[test]
    fn a_local_branch_wins_over_the_origin_one() {
        let origin = BranchPlace::Remote(ORIGIN_REMOTE.to_owned());
        let fork = BranchPlace::Remote("fork".to_owned());
        let branches = [
            branch(BRANCH, fork.clone()),
            branch(BRANCH, origin.clone()),
            branch(BRANCH, BranchPlace::Local),
        ];
        assert_eq!(
            pick_branch(&branches, BRANCH).map(|branch| &branch.place),
            Some(&BranchPlace::Local)
        );
        assert_eq!(
            pick_branch(&branches[..2], BRANCH).map(|branch| &branch.place),
            Some(&origin)
        );
        assert_eq!(
            pick_branch(&branches[..1], BRANCH).map(|branch| &branch.place),
            Some(&fork)
        );
        assert_eq!(pick_branch(&branches, "other"), None);
    }
}
