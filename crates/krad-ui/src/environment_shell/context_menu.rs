mod search;
mod view;

use gpui_kit::{Context, FocusHandle, Window};
use krad_domain::{
    AgentPreset, Effort, EnvironmentData, EnvironmentName, GroupId, PermissionMode, SessionId,
    TerminalId,
};

use super::file_tree::FileAction;
use super::name_dialogs::{DeleteTarget, NameTarget};
use super::surface::SurfaceCommand;
use super::{EnvironmentShell, LiveEnv};
use crate::components::IconName;

const CREATE_WORKTREE_LABEL: &str = "Create worktree";

#[derive(Clone, Debug, PartialEq)]
pub(super) enum MenuAction {
    NewTerminal(AgentPreset),
    NewAgent,
    RemoveWorktree(GroupId),
    RemoveGroup(GroupId),
    RenameEnvironment(EnvironmentName),
    RenameTerminal(TerminalId),
    CloseTerminal(TerminalId),
    SwitchSurface(TerminalId),
    ShowSessionTab(TerminalId),
    DismissLead(SessionId),
    SetEffort(TerminalId, Effort),
    SetPermissionMode(TerminalId, PermissionMode),
    ContinueInTerminal(TerminalId),
    Note(super::notes_view::NoteAction),
    File(FileAction),
    SwitchEnvironment(EnvironmentName),
    NewEnvironment,
    DeleteEnvironment(EnvironmentName),
    RenameGroup(GroupId),
    DeleteGroup(GroupId),
    CustomizeSession(SessionId),
    CloseSession(SessionId),
    SelectGroup(GroupId),
    CreateWorktree(GroupId, String),
    NewGroup,
    OpenSettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuCreate {
    Worktree(GroupId),
}

impl MenuCreate {
    fn label(self, query: &str) -> String {
        match self {
            Self::Worktree(_) => format!("{CREATE_WORKTREE_LABEL} \u{201c}{query}\u{201d}"),
        }
    }

    fn action(self, query: &str) -> MenuAction {
        match self {
            Self::Worktree(group) => MenuAction::CreateWorktree(group, query.to_owned()),
        }
    }
}

#[derive(Clone)]
pub(super) struct Submenu {
    label: String,
    icon: Option<IconName>,
    entries: Vec<MenuEntry>,
    search: Option<&'static str>,
    create: Option<MenuCreate>,
}

#[derive(Clone)]
pub(super) enum MenuEntry {
    Item {
        label: String,
        icon: Option<IconName>,
        action: MenuAction,
        checked: bool,
        destructive: bool,
        detail: Option<String>,
        trailing: Option<String>,
        options: Vec<MenuEntry>,
    },
    Submenu(Submenu),
    Header(String),
    Note(String),
    Separator,
}

impl MenuEntry {
    pub(super) fn item(
        label: impl Into<String>,
        icon: Option<IconName>,
        action: MenuAction,
    ) -> Self {
        Self::Item {
            label: label.into(),
            icon,
            action,
            checked: false,
            destructive: false,
            detail: None,
            trailing: None,
            options: Vec::new(),
        }
    }

    pub(super) fn checked(label: impl Into<String>, checked: bool, action: MenuAction) -> Self {
        Self::Item {
            label: label.into(),
            icon: None,
            action,
            checked,
            destructive: false,
            detail: None,
            trailing: None,
            options: Vec::new(),
        }
    }

    pub(super) fn destructive(
        label: impl Into<String>,
        icon: IconName,
        action: MenuAction,
    ) -> Self {
        Self::Item {
            label: label.into(),
            icon: Some(icon),
            action,
            checked: false,
            destructive: true,
            detail: None,
            trailing: None,
            options: Vec::new(),
        }
    }

    pub(super) fn submenu(
        label: impl Into<String>,
        icon: IconName,
        entries: Vec<MenuEntry>,
    ) -> Self {
        Self::Submenu(Submenu {
            label: label.into(),
            icon: Some(icon),
            entries,
            search: None,
            create: None,
        })
    }

    pub(super) fn with_options(mut self, entries: Vec<MenuEntry>) -> Self {
        if let Self::Item { options, .. } = &mut self {
            *options = entries;
        }
        self
    }

    #[must_use]
    pub(super) fn searchable(mut self, placeholder: &'static str) -> Self {
        if let Self::Submenu(submenu) = &mut self {
            submenu.search = Some(placeholder);
        }
        self
    }

    #[must_use]
    pub(super) fn creating(mut self, create: MenuCreate) -> Self {
        if let Self::Submenu(submenu) = &mut self {
            submenu.create = Some(create);
        }
        self
    }

    #[must_use]
    pub(super) fn with_detail(mut self, text: impl Into<String>) -> Self {
        if let Self::Item { detail, .. } = &mut self {
            *detail = Some(text.into());
        }
        self
    }

    #[must_use]
    pub(super) fn with_trailing(mut self, text: impl Into<String>) -> Self {
        if let Self::Item { trailing, .. } = &mut self {
            *trailing = Some(text.into());
        }
        self
    }
}

#[derive(Clone, Copy)]
pub(super) enum MenuPlacement {
    Below,
    Above,
}

pub(super) struct ContextMenu {
    origin: (f32, f32),
    placement: MenuPlacement,
    entries: Vec<MenuEntry>,
    submenu: Option<usize>,
    options: Option<(Vec<MenuEntry>, (f32, f32))>,
    query: String,
    focus: FocusHandle,
    focus_pending: bool,
}

impl EnvironmentShell {
    pub(super) fn open_menu(
        &mut self,
        origin: (f32, f32),
        placement: MenuPlacement,
        entries: Vec<MenuEntry>,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = Some(ContextMenu {
            origin,
            placement,
            entries,
            submenu: None,
            options: None,
            query: String::new(),
            focus: cx.focus_handle(),
            focus_pending: true,
        });
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.context_menu = None;
        cx.notify();
    }

    pub(super) fn open_menu_options(
        &mut self,
        options: Vec<MenuEntry>,
        origin: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        if let Some(menu) = &mut self.context_menu {
            menu.options = Some((options, origin));
        }
        cx.notify();
    }

    fn close_menu_options(&mut self) {
        if let Some(menu) = &mut self.context_menu {
            menu.options = None;
        }
    }

    pub(super) fn replace_menu_entries(&mut self, entries: Vec<MenuEntry>) {
        if let Some(menu) = &mut self.context_menu {
            menu.entries = entries;
            menu.options = None;
        }
    }

    fn run_menu_action(&mut self, action: MenuAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            MenuAction::RenameEnvironment(name) => {
                self.close_menu_options();
                self.ask_name(NameTarget::Environment(name), window, cx);
                return;
            }
            MenuAction::RenameGroup(group) => {
                self.close_menu_options();
                self.ask_name(NameTarget::Group(group), window, cx);
                return;
            }
            _ => {}
        }
        self.context_menu = None;
        match action {
            MenuAction::DeleteEnvironment(name) => {
                self.ask_delete(DeleteTarget::Environment(name), window, cx);
            }
            MenuAction::DeleteGroup(group) => {
                self.ask_delete(DeleteTarget::Group(group), window, cx);
            }
            MenuAction::CustomizeSession(session) => self.customize_session(session, cx),
            MenuAction::CloseSession(session) => self.close_session(session, cx),
            MenuAction::RenameGroup(_) | MenuAction::RenameEnvironment(_) => {}
            MenuAction::NewTerminal(preset) => self.add_terminal(preset, cx),
            MenuAction::NewAgent => self.open_agent_composer(window, cx),
            MenuAction::RemoveWorktree(group) => {
                self.select_group(group, cx);
                self.remove_selected_worktree(cx);
            }
            MenuAction::RemoveGroup(group) => {
                self.select_group(group, cx);
                self.remove_selected_group(cx);
            }
            MenuAction::RenameTerminal(id) => {
                window.focus(&self.focus, cx);
                self.start_rename(id, cx);
            }
            MenuAction::CloseTerminal(id) => self.close_terminal(id, cx),
            MenuAction::SwitchSurface(id) => self.switch_surface(id, window, cx),
            MenuAction::ShowSessionTab(id) => self.show_terminal_tab(id, window, cx),
            MenuAction::DismissLead(id) => self.dismiss_lead(id, cx),
            MenuAction::SetEffort(id, effort) => {
                self.command_surface(id, SurfaceCommand::Effort(effort), cx);
            }
            MenuAction::SetPermissionMode(owner, mode) => {
                self.set_permission_mode(owner, mode, cx);
            }
            MenuAction::ContinueInTerminal(id) => self.continue_in_terminal(id, cx),
            MenuAction::Note(action) => self.run_note_action(action, cx),
            MenuAction::File(action) => self.run_file_action(action, window, cx),
            MenuAction::SwitchEnvironment(name) => self.switch_env(&name, cx),
            MenuAction::NewEnvironment => self.new_environment(cx),
            MenuAction::SelectGroup(group) => self.select_group(group, cx),
            MenuAction::CreateWorktree(group, name) => self.create_worktree(group, &name, cx),
            MenuAction::NewGroup => Self::prompt_new_group(cx),
            MenuAction::OpenSettings => self.open_settings(window, cx),
        }
        cx.notify();
    }

    fn new_environment(&mut self, cx: &mut Context<Self>) {
        let name = self.unused_env_name();
        let data = EnvironmentData::empty(name.clone());
        self.order.push(name.clone());
        self.envs.insert(name.clone(), LiveEnv::new(data, None));
        self.switch_env(&name, cx);
        Self::prompt_new_group(cx);
    }

    pub(super) fn sync_menu_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = self.context_menu.as_mut().filter(|menu| menu.focus_pending) {
            menu.focus_pending = false;
            window.focus(&menu.focus, cx);
        }
    }
}
