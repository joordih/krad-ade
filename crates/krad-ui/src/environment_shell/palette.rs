use gpui_kit::component::command::{Command, CommandGroup, CommandItem};
use gpui_kit::component::IndexPath;
use gpui_kit::{
    div, px, Action, AnyElement, App, Context, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled, WeakEntity, Window,
};
use krad_domain::{AgentPreset, ChatHit, EnvironmentName, FileOpening, ProjectTask, TerminalId};
use std::path::PathBuf;

mod chat_hits;
mod files;
mod row;
mod tasks;

use chat_hits::chat_hits_group;
use files::file_group;
use row::{command_row, palette_footer, Glyph};
use tasks::task_group;

use super::extensions::{ExtensionPick, PaletteEntry};
use super::notes_view::{note_line_label, NoteLine};
use super::EnvironmentShell;
use crate::actions::{
    NewAgent, NextWaitingAgent, OpenSettings, ToggleActivity, ToggleAttentionInbox,
    ToggleAutomations, ToggleFps, ToggleKraddies, ToggleMinimap, ToggleSidePanel, ToggleSidebar,
    ToggleWorkMode,
};
use crate::components::{harness_icon, IconName};
use crate::theme::{floating_shadow, scrim_fill, Palette, RADIUS_LG};

pub(super) const PALETTE_W: f32 = 620.0;
pub(super) const PALETTE_TOP: f32 = 120.0;

const PALETTE_SECTION_FILES: usize = 0;
const PALETTE_SECTION_NEW: usize = 1;
const PALETTE_SECTION_ENV: usize = 2;
const PALETTE_SECTION_GROUPS: usize = 3;
const PALETTE_SECTION_VIEW: usize = 4;
const PALETTE_SECTION_NOTES: usize = 5;
const PALETTE_SECTION_NOTE_LINES: usize = 6;
const PALETTE_SECTION_CHAT_HITS: usize = 7;
const PALETTE_SECTION_EXTENSIONS: usize = 8;
const PALETTE_SECTION_TASKS: usize = 9;
const PALETTE_GROUP_EXTENSIONS: &str = "Extensions";
const PALETTE_GROUP_NEW: &str = "New terminal in the selected group";
const PALETTE_GROUP_ENV: &str = "Environments";
const PALETTE_GROUP_GROUPS: &str = "Groups and git";
const PALETTE_GROUP_VIEW: &str = "View";
const PALETTE_GROUP_NOTES: &str = "Notes";
const PALETTE_GROUP_NOTE_LINES: &str = "In notes";
const NOTE_SUFFIX: &str = ".md";
const PALETTE_PLACEHOLDER: &str = "Search files, commands, tasks, notes, chats and environments…";
const NOTE_GLYPH: Glyph = (IconName::Note, None);
const NOTE_LINE_GLYPH: Glyph = (IconName::Search, None);

const PALETTE_GROUPS: [(&str, PaletteCmd); 8] = [
    ("Review session changes…", PaletteCmd::SessionChanges),
    ("Run the task as variants…", PaletteCmd::RunVariants),
    ("Compare variants", PaletteCmd::ShowVariants),
    ("New group from a folder…", PaletteCmd::NewGroup),
    ("Remove selected group", PaletteCmd::RemoveGroup),
    ("New worktree from selected group…", PaletteCmd::NewWorktree),
    ("Remove selected worktree", PaletteCmd::RemoveWorktree),
    ("Close file viewer", PaletteCmd::CloseViewer),
];

const PALETTE_VIEW: [(&str, PaletteCmd); 25] = [
    ("Switch work mode", PaletteCmd::ToggleWorkMode),
    ("Select toolchain…", PaletteCmd::SelectToolchain),
    ("Settings…", PaletteCmd::OpenSettings),
    ("Check for updates", PaletteCmd::CheckForUpdates),
    ("New agent with a prompt…", PaletteCmd::NewAgent),
    (
        "New session from a template…",
        PaletteCmd::NewSessionFromTemplate,
    ),
    (
        "Start a session from an issue…",
        PaletteCmd::NewSessionFromIssue,
    ),
    (
        "Go to the next agent waiting for you",
        PaletteCmd::NextWaitingAgent,
    ),
    ("Toggle attention inbox", PaletteCmd::ToggleAttentionInbox),
    ("Toggle automations view", PaletteCmd::ToggleAutomations),
    ("Toggle Kraddies view", PaletteCmd::ToggleKraddies),
    ("Hatch a Kraddie…", PaletteCmd::NewKraddie),
    ("New WSL terminal (bash)", PaletteCmd::NewWslTerminal),
    ("Toggle FPS", PaletteCmd::ToggleFps),
    ("Toggle dark mode", PaletteCmd::ToggleAppearance),
    ("Close focused terminal", PaletteCmd::CloseFocused),
    ("Close environment", PaletteCmd::CloseEnv),
    ("Rename focused terminal", PaletteCmd::RenameFocused),
    ("Rename environment", PaletteCmd::RenameEnv),
    ("Toggle utilities panel", PaletteCmd::ToggleActivity),
    ("Toggle usage bar", PaletteCmd::ToggleUsageBar),
    ("Toggle sidebar", PaletteCmd::ToggleSidebar),
    ("Toggle side panel", PaletteCmd::ToggleSidePanel),
    ("Toggle minimap", PaletteCmd::ToggleMinimap),
    ("Reload workspace", PaletteCmd::ReloadWorkspace),
];

#[derive(Clone, Debug)]
enum PaletteCmd {
    NewTerminal(AgentPreset),
    Goto(EnvironmentName),
    ToggleFps,
    ToggleAppearance,
    NewWslTerminal,
    RenameFocused,
    RenameEnv,
    ToggleActivity,
    ToggleUsageBar,
    ToggleSidebar,
    ToggleSidePanel,
    ToggleMinimap,
    ReloadWorkspace,
    ToggleWorkMode,
    CloseFocused,
    CloseEnv,
    NewGroup,
    RemoveGroup,
    NewWorktree,
    RemoveWorktree,
    CloseViewer,
    SessionChanges,
    RunVariants,
    ShowVariants,
    OpenSettings,
    CheckForUpdates,
    SelectToolchain,
    NewAgent,
    NewSessionFromTemplate,
    NewSessionFromIssue,
    NextWaitingAgent,
    ToggleAttentionInbox,
    ToggleAutomations,
    ToggleKraddies,
    NewKraddie,
    OpenNote(String, Option<u32>),
    OpenFile(PathBuf),
    Extension(ExtensionPick),
    ChatHit(TerminalId, usize),
    RunTask(ProjectTask),
}

struct PaletteRows {
    files: Vec<PathBuf>,
    agents: Vec<AgentPreset>,
    envs: Vec<EnvironmentName>,
    notes: Vec<String>,
    note_lines: Vec<NoteLine>,
    extensions: Vec<PaletteEntry>,
    chat_hits: Vec<ChatHit>,
    tasks: Vec<ProjectTask>,
}

impl PaletteCmd {
    const fn glyph(&self) -> IconName {
        match self {
            Self::NewTerminal(_) | Self::NewWslTerminal => IconName::Terminal,
            Self::Goto(_) => IconName::Home,
            Self::ToggleFps => IconName::Flow,
            Self::ToggleAppearance => IconName::Moon,
            Self::CloseFocused | Self::CloseEnv | Self::CloseViewer => IconName::Close,
            Self::ToggleActivity | Self::ToggleUsageBar => IconName::PanelBottom,
            Self::ToggleSidebar => IconName::SidebarLeft,
            Self::ToggleSidePanel => IconName::SidebarRight,
            Self::ToggleMinimap => IconName::ListView,
            Self::ToggleWorkMode | Self::SelectToolchain | Self::NewSessionFromTemplate => {
                IconName::Layers
            }
            Self::OpenFile(_) => IconName::File,
            Self::NewGroup => IconName::FolderAdd,
            Self::RemoveGroup | Self::RemoveWorktree => IconName::Minus,
            Self::NewWorktree => IconName::GitBranch,
            Self::SessionChanges => IconName::Tasks,
            Self::RunVariants | Self::ShowVariants => IconName::Workflow,
            Self::OpenSettings => IconName::Settings,
            Self::CheckForUpdates | Self::ReloadWorkspace => IconName::Refresh,
            Self::NewAgent => IconName::Magic,
            Self::NewSessionFromIssue => IconName::Flag,
            Self::NextWaitingAgent | Self::ToggleAttentionInbox => IconName::Bell,
            Self::ToggleAutomations => IconName::Flash,
            Self::ToggleKraddies | Self::NewKraddie => IconName::Kraddie,
            Self::RenameFocused | Self::RenameEnv | Self::OpenNote(_, None) => IconName::Note,
            Self::OpenNote(_, Some(_)) => IconName::Search,
            Self::Extension(_) => IconName::Plug,
            Self::ChatHit(..) => IconName::Chat,
            Self::RunTask(_) => IconName::Play,
        }
    }

    fn shortcut(&self) -> Option<Box<dyn Action>> {
        let action: Box<dyn Action> = match self {
            Self::ToggleFps => Box::new(ToggleFps),
            Self::ToggleActivity => Box::new(ToggleActivity),
            Self::ToggleSidebar => Box::new(ToggleSidebar),
            Self::ToggleSidePanel => Box::new(ToggleSidePanel),
            Self::ToggleMinimap => Box::new(ToggleMinimap),
            Self::ToggleWorkMode => Box::new(ToggleWorkMode),
            Self::OpenSettings => Box::new(OpenSettings),
            Self::NewAgent => Box::new(NewAgent),
            Self::NextWaitingAgent => Box::new(NextWaitingAgent),
            Self::ToggleAttentionInbox => Box::new(ToggleAttentionInbox),
            Self::ToggleAutomations => Box::new(ToggleAutomations),
            Self::ToggleKraddies => Box::new(ToggleKraddies),
            _ => return None,
        };
        Some(action)
    }

    fn row(label: &str, cmd: &Self) -> CommandItem {
        command_row(label.to_owned(), (cmd.glyph(), None), cmd.shortcut())
    }

    fn from_path(path: &IndexPath, rows: &PaletteRows) -> Option<Self> {
        let PaletteRows {
            files,
            agents,
            envs,
            notes,
            note_lines,
            extensions,
            chat_hits,
            tasks,
        } = rows;
        for (index, hit) in chat_hits.iter().enumerate() {
            if path.eq_row(IndexPath::new(index).section(PALETTE_SECTION_CHAT_HITS)) {
                return Some(Self::ChatHit(hit.chat, hit.message));
            }
        }
        for (index, task) in tasks.iter().enumerate() {
            if path.eq_row(IndexPath::new(index).section(PALETTE_SECTION_TASKS)) {
                return Some(Self::RunTask(task.clone()));
            }
        }
        for (index, entry) in extensions.iter().enumerate() {
            if path.eq_row(IndexPath::new(index).section(PALETTE_SECTION_EXTENSIONS)) {
                return Some(Self::Extension(entry.pick.clone()));
            }
        }
        for (index, file) in files.iter().enumerate() {
            if path.eq_row(IndexPath::new(index).section(PALETTE_SECTION_FILES)) {
                return Some(Self::OpenFile(file.clone()));
            }
        }
        for (index, note) in notes.iter().enumerate() {
            if path.eq_row(IndexPath::new(index).section(PALETTE_SECTION_NOTES)) {
                return Some(Self::OpenNote(note.clone(), None));
            }
        }
        for (index, line) in note_lines.iter().enumerate() {
            if path.eq_row(IndexPath::new(index).section(PALETTE_SECTION_NOTE_LINES)) {
                return Some(Self::OpenNote(line.path.clone(), Some(line.line)));
            }
        }
        for (index, preset) in agents.iter().enumerate() {
            if path.eq_row(IndexPath::new(index).section(PALETTE_SECTION_NEW)) {
                return Some(Self::NewTerminal(*preset));
            }
        }
        for (index, name) in envs.iter().enumerate() {
            if path.eq_row(IndexPath::new(index).section(PALETTE_SECTION_ENV)) {
                return Some(Self::Goto(name.clone()));
            }
        }
        for (index, entry) in PALETTE_GROUPS.iter().enumerate() {
            if path.eq_row(IndexPath::new(index).section(PALETTE_SECTION_GROUPS)) {
                return Some(entry.1.clone());
            }
        }
        for (index, entry) in PALETTE_VIEW.iter().enumerate() {
            if path.eq_row(IndexPath::new(index).section(PALETTE_SECTION_VIEW)) {
                return Some(entry.1.clone());
            }
        }
        None
    }
}

impl EnvironmentShell {
    pub(super) fn toggle_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_open = !self.palette_open;
        if self.palette_open {
            self.palette_files.clear();
            self.index_palette_files(cx);
            self.refresh_chat_index(cx);
            self.detect_palette_tasks(cx);
            if let Some(palette) = &self.palette {
                palette.update(cx, |state, cx| {
                    state.set_query("", window, cx);
                    state.focus(window, cx);
                });
            }
        }
        cx.notify();
    }

    pub(super) fn dismiss_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette_open {
            self.palette_open = false;
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    fn apply_palette(&mut self, cmd: PaletteCmd, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_open = false;
        match cmd {
            PaletteCmd::NewTerminal(preset) => self.add_terminal(preset, cx),
            PaletteCmd::Goto(name) => self.switch_env(&name, cx),
            PaletteCmd::ToggleFps => {
                self.fps_hud = self.fps_hud.toggle();
                cx.notify();
            }
            PaletteCmd::ToggleAppearance => self.toggle_appearance(cx),
            PaletteCmd::NewWslTerminal => self.add_wsl_terminal(cx),
            PaletteCmd::RenameFocused => self.start_rename_focused(cx),
            PaletteCmd::RenameEnv => {
                let active = self.active.clone();
                self.start_rename_env(&active, cx);
            }
            PaletteCmd::ToggleActivity => self.toggle_activity(cx),
            PaletteCmd::ToggleUsageBar => self.toggle_usage_bar(cx),
            PaletteCmd::ToggleSidebar => self.toggle_sidebar(cx),
            PaletteCmd::ToggleSidePanel => self.toggle_side_panel(cx),
            PaletteCmd::ToggleMinimap => self.toggle_minimap(cx),
            PaletteCmd::ReloadWorkspace => self.reload_workspace(cx),
            PaletteCmd::ToggleWorkMode => self.toggle_work_mode(cx),
            PaletteCmd::OpenFile(path) => self.open_file_tab(&path, FileOpening::Pinned, cx),
            PaletteCmd::CloseFocused => self.close_focused(window, cx),
            PaletteCmd::CloseEnv => {
                let active = self.active.clone();
                self.close_env(&active, cx);
            }
            PaletteCmd::NewGroup => self.prompt_new_group(window, cx),
            PaletteCmd::RemoveGroup => self.remove_selected_group(cx),
            PaletteCmd::NewWorktree => self.start_new_worktree(cx),
            PaletteCmd::RemoveWorktree => self.remove_selected_worktree(cx),
            PaletteCmd::CloseViewer => self.close_viewer(cx),
            PaletteCmd::SessionChanges => self.open_session_changes(cx),
            PaletteCmd::RunVariants => self.open_variants_draft(window, cx),
            PaletteCmd::ShowVariants => self.open_variants_view(cx),
            PaletteCmd::OpenSettings => self.open_settings(window, cx),
            PaletteCmd::CheckForUpdates => self.ask_for_app_update(cx),
            PaletteCmd::SelectToolchain => self.select_focused_toolchain(cx),
            PaletteCmd::NewAgent => self.open_agent_composer(window, cx),
            PaletteCmd::NewSessionFromTemplate => self.open_template_picker(cx),
            PaletteCmd::NewSessionFromIssue => self.open_issue_picker(cx),
            PaletteCmd::NextWaitingAgent => self.open_next_waiting(window, cx),
            PaletteCmd::ToggleAttentionInbox => self.toggle_attention_inbox(cx),
            PaletteCmd::ToggleAutomations => self.toggle_automations(cx),
            PaletteCmd::ToggleKraddies => self.toggle_kraddies(cx),
            PaletteCmd::NewKraddie => self.open_kraddie_editor(None, window, cx),
            PaletteCmd::OpenNote(path, line) => self.open_note_from_palette(&path, line, cx),
            PaletteCmd::Extension(pick) => self.apply_extension_pick(pick, cx),
            PaletteCmd::ChatHit(id, message) => self.open_chat_hit_menu(id, message, window, cx),
            PaletteCmd::RunTask(task) => self.run_project_task(task, cx),
        }
    }

    fn close_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let focused = self.envs.get(&env).and_then(|live| live.focused);
        if let Some(id) = focused {
            self.close_tab_asking(id, window, cx);
        }
    }

    pub(super) fn palette_overlay(
        &self,
        vp_w: f32,
        pal: &Palette,
        cx: &gpui_kit::App,
    ) -> AnyElement {
        let (Some(state), Some(weak)) = (self.palette.clone(), self.self_ref.clone()) else {
            return div().into_any_element();
        };
        if !self.palette_open {
            return div().into_any_element();
        }
        let query = state.read(cx).query(cx).to_string();
        let root = self.palette_root();
        let rows = PaletteRows {
            files: self.palette_files.clone(),
            agents: self.installed_agents().to_vec(),
            envs: self.order.clone(),
            notes: self.palette_notes(&query),
            note_lines: self.palette_note_lines().to_vec(),
            extensions: self.extension_palette_entries(),
            chat_hits: self.palette_chat_hits().to_vec(),
            tasks: self.palette_tasks.clone(),
        };
        let weak_cancel = weak.clone();
        let weak_query = weak.clone();
        let weak_backdrop = weak.clone();
        let left = ((vp_w - PALETTE_W) / 2.0).max(0.0);
        let command =
            Command::new(&state)
                .group(file_group(&rows.files, root.as_deref(), &query))
                .group(
                    CommandGroup::new()
                        .label(PALETTE_GROUP_NEW)
                        .items(rows.agents.iter().map(|preset| {
                            let (glyph, color) = harness_icon(*preset, pal);
                            command_row(
                                self.settings.acp.harness_name(*preset),
                                (glyph, Some(color)),
                                None,
                            )
                        })),
                )
                .group(CommandGroup::new().label(PALETTE_GROUP_ENV).items(
                    rows.envs.iter().map(|name| {
                        command_row(self.env_label(name), (IconName::Home, None), None)
                    }),
                ))
                .group(
                    CommandGroup::new().label(PALETTE_GROUP_GROUPS).items(
                        PALETTE_GROUPS
                            .iter()
                            .map(|(label, cmd)| PaletteCmd::row(label, cmd)),
                    ),
                )
                .group(
                    CommandGroup::new().label(PALETTE_GROUP_VIEW).items(
                        PALETTE_VIEW
                            .iter()
                            .map(|(label, cmd)| PaletteCmd::row(label, cmd)),
                    ),
                )
                .group(note_group(&rows.notes, &query))
                .group(note_lines_group(&rows.note_lines, &query))
                .group(chat_hits_group(&rows.chat_hits, &query))
                .group(extension_group(&rows.extensions))
                .group(task_group(&rows.tasks))
                .placeholder(PALETTE_PLACEHOLDER)
                .footer(|_, _, cx| palette_footer(cx))
                .on_query(move |query: &str, _: &mut Window, cx: &mut App| {
                    weak_query
                        .update(cx, |shell, cx| {
                            shell.find_palette_files(query);
                            shell.search_notes(query, cx);
                            shell.search_chat_history(query, cx);
                        })
                        .unwrap_or_default();
                })
                .on_confirm(move |path: IndexPath, window: &mut Window, cx: &mut App| {
                    if let Some(cmd) = PaletteCmd::from_path(&path, &rows) {
                        let handle = weak.update(cx, |shell, cx| {
                            shell.apply_palette(cmd, window, cx);
                            cx.notify();
                            shell.focus.clone()
                        });
                        if let Ok(handle) = handle {
                            window.focus(&handle, cx);
                        }
                    }
                })
                .on_cancel(move |window: &mut Window, cx: &mut App| {
                    dismiss(&weak_cancel, window, cx);
                });
        palette_frame(command, left, pal, weak_backdrop, cx)
    }
}

fn extension_group(entries: &[PaletteEntry]) -> CommandGroup {
    CommandGroup::new().label(PALETTE_GROUP_EXTENSIONS).items(
        entries
            .iter()
            .map(|entry| command_row(entry.label.clone(), (entry.icon, None), entry.shortcut())),
    )
}

fn note_group(notes: &[String], query: &str) -> CommandGroup {
    CommandGroup::new()
        .label(PALETTE_GROUP_NOTES)
        .items(notes.iter().map(|note| {
            command_row(
                note.strip_suffix(NOTE_SUFFIX).unwrap_or(note).to_owned(),
                NOTE_GLYPH,
                None,
            )
            .keywords([query.to_owned()])
        }))
}

fn note_lines_group(lines: &[NoteLine], query: &str) -> CommandGroup {
    CommandGroup::new()
        .label(PALETTE_GROUP_NOTE_LINES)
        .items(lines.iter().map(|line| {
            command_row(note_line_label(line), NOTE_LINE_GLYPH, None).keywords([query.to_owned()])
        }))
}

fn palette_frame(
    command: Command,
    left: f32,
    pal: &Palette,
    backdrop: WeakEntity<EnvironmentShell>,
    cx: &App,
) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .child(
            div()
                .absolute()
                .inset_0()
                .bg(scrim_fill(pal, cx))
                .occlude()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    dismiss(&backdrop, window, cx);
                }),
        )
        .child(
            div()
                .absolute()
                .left(px(left))
                .top(px(PALETTE_TOP))
                .w(px(PALETTE_W))
                .rounded(px(RADIUS_LG))
                .overflow_hidden()
                .shadow(floating_shadow(pal))
                .occlude()
                .child(command),
        )
        .into_any_element()
}

fn dismiss(shell: &WeakEntity<EnvironmentShell>, window: &mut Window, cx: &mut App) {
    shell
        .update(cx, |shell, cx| shell.dismiss_palette(window, cx))
        .unwrap_or_default();
}
