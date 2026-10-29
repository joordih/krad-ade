use gpui_kit::{Context, MouseDownEvent, Pixels, Point};
use krad_domain::{GroupId, TerminalId};

use super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::EnvironmentShell;
use crate::components::IconName;

const RENAME_LABEL: &str = "Rename";
const CLOSE_TERMINAL_LABEL: &str = "Close terminal";
const REMOVE_WORKTREE_LABEL: &str = "Remove worktree";
const REMOVE_GROUP_LABEL: &str = "Remove group";
const NEW_TERMINAL_HEADER: &str = "Open with";
const NEW_AGENT_LABEL: &str = "New agent with a prompt…";

fn point(position: Point<Pixels>) -> (f32, f32) {
    (position.x.as_f32(), position.y.as_f32())
}

impl EnvironmentShell {
    pub(super) fn open_menu_at(
        &mut self,
        event: &MouseDownEvent,
        entries: Vec<MenuEntry>,
        cx: &mut Context<Self>,
    ) {
        self.open_menu(point(event.position), MenuPlacement::Below, entries, cx);
    }

    pub(super) fn open_harness_menu(
        &mut self,
        anchor: Point<Pixels>,
        placement: MenuPlacement,
        cx: &mut Context<Self>,
    ) {
        let mut entries = vec![
            MenuEntry::item(
                NEW_AGENT_LABEL,
                Some(IconName::UserGroup),
                MenuAction::NewAgent,
            ),
            MenuEntry::Separator,
            MenuEntry::Header(NEW_TERMINAL_HEADER.to_owned()),
        ];
        entries.extend(self.installed_agents().iter().map(|preset| {
            MenuEntry::checked(
                preset.display_name(),
                *preset == self.settings.default_harness,
                MenuAction::NewTerminal(*preset),
            )
        }));
        self.open_menu(point(anchor), placement, entries, cx);
    }

    pub(super) fn group_menu(&self, id: GroupId) -> Vec<MenuEntry> {
        let group = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.group(id));
        let is_worktree = group.is_some_and(|group| group.worktree.is_some());
        let mut entries = Vec::new();
        if is_worktree {
            entries.push(MenuEntry::destructive(
                REMOVE_WORKTREE_LABEL,
                IconName::GitBranch,
                MenuAction::RemoveWorktree(id),
            ));
        }
        entries.push(MenuEntry::destructive(
            REMOVE_GROUP_LABEL,
            IconName::Close,
            MenuAction::RemoveGroup(id),
        ));
        entries
    }

    pub(super) fn terminal_menu(id: TerminalId) -> Vec<MenuEntry> {
        vec![
            MenuEntry::item(RENAME_LABEL, None, MenuAction::RenameTerminal(id)),
            MenuEntry::Separator,
            MenuEntry::destructive(
                CLOSE_TERMINAL_LABEL,
                IconName::Close,
                MenuAction::CloseTerminal(id),
            ),
        ]
    }
}
