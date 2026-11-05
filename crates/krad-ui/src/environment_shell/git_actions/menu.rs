use gpui_kit::Context;

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::EnvironmentShell;
use super::{CommitIntent, GitAction};
use crate::components::IconName;

const PULL_LABEL: &str = "Pull";
const PUSH_LABEL: &str = "Push";
const FETCH_LABEL: &str = "Fetch";
const SYNC_LABEL: &str = "Pull, Push";
const CHECKOUT_LABEL: &str = "Checkout to…";
const CREATE_BRANCH_LABEL: &str = "Create branch…";
const COMMIT_LABEL: &str = "Commit";
const COMMIT_PUSH_LABEL: &str = "Commit & Push";
const AMEND_LABEL: &str = "Commit (Amend)";
const STAGE_ALL_LABEL: &str = "Stage all changes";
const UNSTAGE_ALL_LABEL: &str = "Unstage all changes";
const STASH_LABEL: &str = "Stash";
const POP_STASH_LABEL: &str = "Pop stash";
const TAGS_LABEL: &str = "Tags";
const CREATE_TAG_LABEL: &str = "Create tag…";
const PUSH_TAGS_LABEL: &str = "Push tags";
const DELETE_TAG_HEADER: &str = "Delete a tag";
const COMMIT_SUBMENU_LABEL: &str = "Commit";
const CHANGES_SUBMENU_LABEL: &str = "Changes";
const STASH_SUBMENU_LABEL: &str = "Stash";

fn git_item(label: &str, action: GitAction) -> MenuEntry {
    MenuEntry::item(label, None, MenuAction::Git(action))
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_git_menu(
        &mut self,
        origin: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        self.git_ui.menu_origin = origin;
        let entries = self.git_menu_entries();
        self.open_menu(origin, MenuPlacement::Below, entries, cx);
    }

    fn git_menu_entries(&self) -> Vec<MenuEntry> {
        let tags = self.group_git().map_or_default(|git| git.refs.tags.clone());
        let mut tag_entries = vec![
            git_item(CREATE_TAG_LABEL, GitAction::CreateTag),
            git_item(PUSH_TAGS_LABEL, GitAction::PushTags),
        ];
        if !tags.is_empty() {
            tag_entries.push(MenuEntry::Header(DELETE_TAG_HEADER.to_owned()));
            tag_entries.extend(tags.into_iter().map(|tag| {
                MenuEntry::destructive(
                    tag.clone(),
                    IconName::Close,
                    MenuAction::Git(GitAction::DeleteTag(tag)),
                )
            }));
        }
        vec![
            git_item(PULL_LABEL, GitAction::Pull),
            git_item(PUSH_LABEL, GitAction::Push),
            git_item(FETCH_LABEL, GitAction::Fetch),
            git_item(SYNC_LABEL, GitAction::Sync),
            MenuEntry::Separator,
            git_item(CHECKOUT_LABEL, GitAction::CheckoutTo),
            git_item(CREATE_BRANCH_LABEL, GitAction::CreateBranch),
            MenuEntry::Separator,
            MenuEntry::submenu(
                COMMIT_SUBMENU_LABEL,
                IconName::Tick,
                vec![
                    git_item(COMMIT_LABEL, GitAction::Commit(CommitIntent::Commit)),
                    git_item(
                        COMMIT_PUSH_LABEL,
                        GitAction::Commit(CommitIntent::CommitAndPush),
                    ),
                    git_item(AMEND_LABEL, GitAction::Commit(CommitIntent::Amend)),
                ],
            ),
            MenuEntry::submenu(
                CHANGES_SUBMENU_LABEL,
                IconName::File,
                vec![
                    git_item(STAGE_ALL_LABEL, GitAction::StageAll),
                    git_item(UNSTAGE_ALL_LABEL, GitAction::UnstageAll),
                ],
            ),
            MenuEntry::submenu(
                STASH_SUBMENU_LABEL,
                IconName::Layers,
                vec![
                    git_item(STASH_LABEL, GitAction::Stash),
                    git_item(POP_STASH_LABEL, GitAction::PopStash),
                ],
            ),
            MenuEntry::submenu(TAGS_LABEL, IconName::Flag, tag_entries),
        ]
    }
}
