use gpui_kit::component::command::{Command, CommandGroup, CommandItem};
use gpui_kit::component::IndexPath;
use gpui_kit::{
    div, px, Action, AnyElement, App, Context, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled, WeakEntity, Window,
};
use krad_domain::{AgentPreset, EnvironmentName};

mod row;

use row::{command_row, palette_footer, Glyph};

use super::notes_view::{note_line_label, NoteLine};
use super::EnvironmentShell;
use crate::actions::{
    NewAgent, OpenSettings, ToggleActivity, ToggleFps, ToggleSidePanel, ToggleSidebar,
};
use crate::components::{harness_icon, IconName};
use crate::theme::{floating_shadow, scrim_fill, Palette, RADIUS_LG};

const PALETTE_W: f32 = 620.0;
const PALETTE_TOP: f32 = 120.0;

const PALETTE_SECTION_NEW: usize = 0;
const PALETTE_SECTION_ENV: usize = 1;
const PALETTE_SECTION_GROUPS: usize = 2;
const PALETTE_SECTION_VIEW: usize = 3;
const PALETTE_SECTION_NOTES: usize = 4;
const PALETTE_SECTION_NOTE_LINES: usize = 5;
const PALETTE_GROUP_NEW: &str = "New terminal in the selected group";
const PALETTE_GROUP_ENV: &str = "Environments";
const PALETTE_GROUP_GROUPS: &str = "Groups and git";
const PALETTE_GROUP_VIEW: &str = "View";
const PALETTE_GROUP_NOTES: &str = "Notes";
const PALETTE_GROUP_NOTE_LINES: &str = "In notes";
const NOTE_SUFFIX: &str = ".md";
const PALETTE_PLACEHOLDER: &str = "Search commands, notes and environments…";
const NOTE_GLYPH: Glyph = (IconName::Note, None);
const NOTE_LINE_GLYPH: Glyph = (IconName::Search, None);

const PALETTE_GROUPS: [(&str, PaletteCmd); 5] = [
    ("New group from a folder…", PaletteCmd::NewGroup),
    ("Remove selected group", PaletteCmd::RemoveGroup),
    ("New worktree from selected group…", PaletteCmd::NewWorktree),
    ("Remove selected worktree", PaletteCmd::RemoveWorktree),
    ("Close file viewer", PaletteCmd::CloseViewer),
];

const PALETTE_VIEW: [(&str, PaletteCmd); 12] = [
    ("Settings…", PaletteCmd::OpenSettings),
    ("New agent with a prompt…", PaletteCmd::NewAgent),
    ("New WSL terminal (bash)", PaletteCmd::NewWslTerminal),
    ("Toggle FPS", PaletteCmd::ToggleFps),
    ("Toggle dark mode", PaletteCmd::ToggleAppearance),
    ("Close focused terminal", PaletteCmd::CloseFocused),
    ("Close environment", PaletteCmd::CloseEnv),
    ("Rename focused terminal", PaletteCmd::RenameFocused),
    ("Rename environment", PaletteCmd::RenameEnv),
    ("Toggle utilities panel", PaletteCmd::ToggleActivity),
    ("Toggle sidebar", PaletteCmd::ToggleSidebar),
    ("Toggle side panel", PaletteCmd::ToggleSidePanel),
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
    ToggleSidebar,
    ToggleSidePanel,
    CloseFocused,
    CloseEnv,
    NewGroup,
    RemoveGroup,
    NewWorktree,
    RemoveWorktree,
    CloseViewer,
    OpenSettings,
    NewAgent,
    OpenNote(String, Option<u32>),
}

struct PaletteRows {
    agents: Vec<AgentPreset>,
    envs: Vec<EnvironmentName>,
    notes: Vec<String>,
    note_lines: Vec<NoteLine>,
}

impl PaletteCmd {
    const fn glyph(&self) -> IconName {
        match self {
            Self::NewTerminal(_) | Self::NewWslTerminal => IconName::Terminal,
            Self::Goto(_) => IconName::Home,
            Self::ToggleFps => IconName::Flow,
            Self::ToggleAppearance => IconName::Moon,
            Self::CloseFocused | Self::CloseEnv | Self::CloseViewer => IconName::Close,
            Self::ToggleActivity => IconName::PanelBottom,
            Self::ToggleSidebar => IconName::SidebarLeft,
            Self::ToggleSidePanel => IconName::SidebarRight,
            Self::NewGroup => IconName::FolderAdd,
            Self::RemoveGroup | Self::RemoveWorktree => IconName::Minus,
            Self::NewWorktree => IconName::GitBranch,
            Self::OpenSettings => IconName::Settings,
            Self::NewAgent => IconName::Magic,
            Self::RenameFocused | Self::RenameEnv | Self::OpenNote(_, None) => IconName::Note,
            Self::OpenNote(_, Some(_)) => IconName::Search,
        }
    }

    fn shortcut(&self) -> Option<Box<dyn Action>> {
        let action: Box<dyn Action> = match self {
            Self::ToggleFps => Box::new(ToggleFps),
            Self::ToggleActivity => Box::new(ToggleActivity),
            Self::ToggleSidebar => Box::new(ToggleSidebar),
            Self::ToggleSidePanel => Box::new(ToggleSidePanel),
            Self::OpenSettings => Box::new(OpenSettings),
            Self::NewAgent => Box::new(NewAgent),
            _ => return None,
        };
        Some(action)
    }

    fn row(label: &str, cmd: &Self) -> CommandItem {
        command_row(label.to_owned(), (cmd.glyph(), None), cmd.shortcut())
    }

    fn from_path(path: &IndexPath, rows: &PaletteRows) -> Option<Self> {
        let PaletteRows {
            agents,
            envs,
            notes,
            note_lines,
        } = rows;
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
            PaletteCmd::ToggleSidebar => self.toggle_sidebar(cx),
            PaletteCmd::ToggleSidePanel => self.toggle_side_panel(cx),
            PaletteCmd::CloseFocused => self.close_focused(cx),
            PaletteCmd::CloseEnv => {
                let active = self.active.clone();
                self.close_env(&active, cx);
            }
            PaletteCmd::NewGroup => Self::prompt_new_group(cx),
            PaletteCmd::RemoveGroup => self.remove_selected_group(cx),
            PaletteCmd::NewWorktree => self.start_new_worktree(cx),
            PaletteCmd::RemoveWorktree => self.remove_selected_worktree(cx),
            PaletteCmd::CloseViewer => self.close_viewer(cx),
            PaletteCmd::OpenSettings => self.open_settings(window, cx),
            PaletteCmd::NewAgent => self.open_agent_composer(window, cx),
            PaletteCmd::OpenNote(path, line) => self.open_note_from_palette(&path, line, cx),
        }
    }

    fn close_focused(&mut self, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let focused = self.envs.get(&env).and_then(|live| live.focused);
        if let Some(id) = focused {
            self.close_terminal(id, cx);
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
        let rows = PaletteRows {
            agents: self.installed_agents().to_vec(),
            envs: self.order.clone(),
            notes: self.palette_notes(&query),
            note_lines: self.palette_note_lines().to_vec(),
        };
        let weak_cancel = weak.clone();
        let weak_query = weak.clone();
        let weak_backdrop = weak.clone();
        let left = ((vp_w - PALETTE_W) / 2.0).max(0.0);
        let command =
            Command::new(&state)
                .group(
                    CommandGroup::new()
                        .label(PALETTE_GROUP_NEW)
                        .items(rows.agents.iter().map(|preset| {
                            let (glyph, color) = harness_icon(*preset, pal);
                            command_row(preset.display_name(), (glyph, Some(color)), None)
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
                .placeholder(PALETTE_PLACEHOLDER)
                .footer(|_, _, cx| palette_footer(cx))
                .on_query(move |query: &str, _: &mut Window, cx: &mut App| {
                    weak_query
                        .update(cx, |shell, cx| shell.search_notes(query, cx))
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
                        dismiss(&weak_backdrop, window, cx);
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

fn dismiss(shell: &WeakEntity<EnvironmentShell>, window: &mut Window, cx: &mut App) {
    shell
        .update(cx, |shell, cx| shell.dismiss_palette(window, cx))
        .unwrap_or_default();
}
