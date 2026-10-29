use gpui_kit::Context;
use krad_domain::{GroupId, TerminalGroup};
use std::path::PathBuf;

use super::context_menu::{MenuAction, MenuCreate, MenuEntry, MenuPlacement};
use super::EnvironmentShell;
use crate::components::IconName;
use crate::keymap::OPEN_SETTINGS_HINT;

const ENVIRONMENT_LABEL: &str = "Environment";
const SEARCH_ENVIRONMENTS: &str = "Search environments…";
const NEW_ENVIRONMENT_LABEL: &str = "New environment";
const GROUPS_LABEL: &str = "Groups";
const SEARCH_GROUPS: &str = "Search groups…";
const NEW_GROUP_LABEL: &str = "New group…";
const RENAME_LABEL: &str = "Rename";
const DELETE_LABEL: &str = "Delete";
const SWITCH_WORKTREE_LABEL: &str = "Switch worktree";
const SEARCH_WORKTREES: &str = "Search or create a worktree…";
const NOT_A_REPO_TEXT: &str = "This group's folder is not a git repository.";
const NO_GROUP_TEXT: &str = "Select a group first.";
const SETTINGS_LABEL: &str = "Settings";
const DETACHED_LABEL: &str = "detached";
const TERMINALS_SUFFIX: &str = "terminals";
const NEW_ACTIVITY_SUFFIX: &str = "new activity";

impl EnvironmentShell {
    pub(super) fn open_environment_menu(
        &mut self,
        origin: (f32, f32),
        placement: MenuPlacement,
        cx: &mut Context<Self>,
    ) {
        let entries = self.switcher_menu_entries();
        self.open_menu(origin, placement, entries, cx);
    }

    pub(super) fn switcher_menu_entries(&self) -> Vec<MenuEntry> {
        let group = self.active_group().map(|group| group.id);
        vec![
            self.worktree_submenu(group),
            MenuEntry::Separator,
            self.environment_submenu(),
            self.groups_submenu(group),
            MenuEntry::item(
                SETTINGS_LABEL,
                Some(IconName::Settings),
                MenuAction::OpenSettings,
            )
            .with_trailing(OPEN_SETTINGS_HINT),
        ]
    }

    fn environment_submenu(&self) -> MenuEntry {
        let mut entries: Vec<MenuEntry> = self
            .order
            .iter()
            .map(|name| {
                let count = self.envs.get(name).map_or(0, |live| live.data.nodes.len());
                let detail = if self.unseen.contains(name) {
                    format!("{count} {TERMINALS_SUFFIX} · {NEW_ACTIVITY_SUFFIX}")
                } else {
                    format!("{count} {TERMINALS_SUFFIX}")
                };
                MenuEntry::checked(
                    self.env_label(name),
                    *name == self.active,
                    MenuAction::SwitchEnvironment(name.clone()),
                )
                .with_detail(detail)
                .with_options(vec![
                    MenuEntry::item(
                        RENAME_LABEL,
                        None,
                        MenuAction::RenameEnvironment(name.clone()),
                    ),
                    MenuEntry::destructive(
                        DELETE_LABEL,
                        IconName::Close,
                        MenuAction::DeleteEnvironment(name.clone()),
                    ),
                ])
            })
            .collect();
        entries.extend([
            MenuEntry::Separator,
            MenuEntry::item(
                NEW_ENVIRONMENT_LABEL,
                Some(IconName::Add),
                MenuAction::NewEnvironment,
            ),
        ]);
        MenuEntry::submenu(ENVIRONMENT_LABEL, IconName::Layers, entries)
            .searchable(SEARCH_ENVIRONMENTS)
    }

    fn groups_submenu(&self, selected: Option<GroupId>) -> MenuEntry {
        let mut entries: Vec<MenuEntry> = self.envs.get(&self.active).map_or_default(|live| {
            live.data
                .groups
                .iter()
                .map(|group| {
                    let sessions = live.data.sessions_of(Some(group.id)).count();
                    MenuEntry::checked(
                        group.name.clone(),
                        selected == Some(group.id),
                        MenuAction::SelectGroup(group.id),
                    )
                    .with_trailing(sessions.to_string())
                    .with_options(vec![
                        MenuEntry::item(RENAME_LABEL, None, MenuAction::RenameGroup(group.id)),
                        MenuEntry::destructive(
                            DELETE_LABEL,
                            IconName::Close,
                            MenuAction::DeleteGroup(group.id),
                        ),
                    ])
                })
                .collect()
        });
        if !entries.is_empty() {
            entries.push(MenuEntry::Separator);
        }
        entries.push(MenuEntry::item(
            NEW_GROUP_LABEL,
            Some(IconName::FolderAdd),
            MenuAction::NewGroup,
        ));
        MenuEntry::submenu(GROUPS_LABEL, IconName::Folder, entries).searchable(SEARCH_GROUPS)
    }

    fn worktree_submenu(&self, selected: Option<GroupId>) -> MenuEntry {
        let submenu = |entries| {
            MenuEntry::submenu(SWITCH_WORKTREE_LABEL, IconName::GitBranch, entries)
                .searchable(SEARCH_WORKTREES)
        };
        let Some(live) = self.envs.get(&self.active) else {
            return submenu(Vec::new());
        };
        let Some(group) = selected.and_then(|id| live.data.group(id)) else {
            return submenu(vec![MenuEntry::Note(NO_GROUP_TEXT.to_owned())]);
        };
        let Some(repo) = self.repo_of(group) else {
            return submenu(vec![MenuEntry::Note(NOT_A_REPO_TEXT.to_owned())]);
        };
        let entries = live
            .data
            .groups
            .iter()
            .filter(|other| self.repo_of(other).as_ref() == Some(&repo))
            .map(|other| {
                let branch = self
                    .git
                    .get(&other.root)
                    .and_then(|git| git.status.branch.clone())
                    .or_else(|| other.worktree.as_ref().map(|origin| origin.branch.clone()))
                    .unwrap_or_else(|| DETACHED_LABEL.to_owned());
                let sessions = live.data.sessions_of(Some(other.id)).count();
                MenuEntry::checked(
                    other.name.clone(),
                    other.id == group.id,
                    MenuAction::SelectGroup(other.id),
                )
                .with_detail(branch)
                .with_trailing(sessions.to_string())
            })
            .collect();
        submenu(entries).creating(MenuCreate::Worktree(group.id))
    }

    fn repo_of(&self, group: &TerminalGroup) -> Option<PathBuf> {
        group
            .worktree
            .as_ref()
            .map(|origin| origin.repo_root.clone())
            .or_else(|| self.git.get(&group.root).map(|git| git.repo_root.clone()))
    }
}
