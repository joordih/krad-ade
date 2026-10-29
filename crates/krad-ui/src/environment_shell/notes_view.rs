mod agents;
mod editor;
mod format_menu;
mod graph_view;
mod inspector;
mod markdown;
mod palette;
mod reading;
mod sidebar;
mod tabs_bar;
mod tree;

pub(super) use palette::{note_line_label, NoteLine};

use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, IntoElement, ParentElement, Styled, Task,
};
use krad_domain::{FileOpening, NoteMode, NoteTabs, VaultIndex, VAULT_DIR};
use krad_repo::{
    create_note, create_vault_folder, stamp_new_notes, vault_entries, vault_notes, vault_path,
    VaultEntry,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::EnvironmentShell;
use crate::components::{card, Button, ButtonVariant, EmptyState, IconName};
use crate::theme::{palette, WINDOW_INSET};

pub(super) use editor::NoteEditor;
pub(super) use inspector::InspectorTab;

const VAULT_POLL: Duration = Duration::from_secs(2);
const NEW_NOTE_TITLE: &str = "Untitled";
const NEW_FOLDER_NAME: &str = "New folder";
const NO_GROUP_TITLE: &str = "No group selected";
const NO_GROUP_TEXT: &str =
    "Notes live in a group's repository, in docs/notes. Pick a group in the sidebar.";
const NO_NOTE_TITLE: &str = "No note open";
const NO_NOTE_TEXT: &str = "Pick one in the sidebar, or find it from the command palette.";
const NEW_NOTE_LABEL: &str = "New note";
const BTN_EMPTY_NEW_NOTE: &str = "notes-empty-new-note";
const SPLIT_RULE_W: f32 = 1.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum NoteAction {
    NewNote(String),
    NewFolder(String),
    Open(String),
    NewNamed(String),
}

#[derive(Default)]
pub(super) struct NotesUi {
    root: Option<PathBuf>,
    entries: Vec<VaultEntry>,
    pub(super) index: VaultIndex,
    expanded: HashSet<String>,
    selected: Option<String>,
    pub(super) editors: HashMap<String, NoteEditor>,
    loading: bool,
    poll: Option<Task<()>>,
    pub(super) reading: HashMap<String, reading::ReadingView>,
    pub(super) pending_heading: Option<(String, usize)>,
    pub(super) property_edit: Option<inspector::PropertyEdit>,
    pub(super) tag_filter: Option<String>,
    pub(super) inspector: InspectorTab,
    pub(super) inspector_open: bool,
    pub(super) graph: graph_view::GraphState,
    search: palette::NotesSearch,
    sidebar: sidebar::NotesSidebar,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

struct VaultRead {
    entries: Vec<VaultEntry>,
    index: VaultIndex,
}

fn read_vault(root: &Path) -> Result<VaultRead, krad_repo::RepoError> {
    stamp_new_notes(root, now_secs())?;
    Ok(VaultRead {
        entries: vault_entries(root)?,
        index: VaultIndex::new(vault_notes(root)?),
    })
}

impl EnvironmentShell {
    fn notes_root(&self) -> Option<PathBuf> {
        self.active_group().map(|group| group.root.clone())
    }

    pub(super) fn note_tabs(&self) -> Option<&NoteTabs> {
        let group = self.active_group()?.id;
        self.envs
            .get(&self.active)?
            .data
            .note_tabs
            .iter()
            .find(|tabs| tabs.group == Some(group))
    }

    pub(super) fn edit_note_tabs(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut NoteTabs),
    ) {
        let Some(group) = self.active_group().map(|group| group.id) else {
            return;
        };
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let index = live
            .data
            .note_tabs
            .iter()
            .position(|tabs| tabs.group == Some(group))
            .unwrap_or_else(|| {
                live.data.note_tabs.push(NoteTabs::for_group(group));
                live.data.note_tabs.len() - 1
            });
        let Some(tabs) = live.data.note_tabs.get_mut(index) else {
            return;
        };
        edit(tabs);
        cx.notify();
        self.store();
    }

    pub(super) fn vault_file(&self, path: &str) -> Option<PathBuf> {
        vault_path(&self.notes.root.clone()?, path)
    }

    fn sync_vault(&mut self, cx: &mut Context<Self>) {
        let root = self.notes_root();
        if self.notes.root != root {
            self.notes = NotesUi {
                root: root.clone(),
                sidebar: std::mem::take(&mut self.notes.sidebar),
                ..NotesUi::default()
            };
            if let Some(root) = root {
                self.read_vault_now(root, cx);
            }
        }
        if self.notes.poll.is_none() {
            let task = cx.spawn(async move |this, cx| loop {
                cx.background_executor().timer(VAULT_POLL).await;
                let root = this
                    .update(cx, |shell, _| {
                        let showing = shell.active_view() == krad_domain::EnvironmentView::Notes;
                        showing.then(|| shell.notes.root.clone()).flatten()
                    })
                    .ok();
                let Some(root) = root else {
                    break;
                };
                if let Some(root) = root {
                    let read = cx.background_spawn(async move { read_vault(&root) }).await;
                    let applied = this.update(cx, |shell, cx| shell.apply_vault(read, cx));
                    if applied.is_err() {
                        break;
                    }
                }
            });
            self.notes.poll = Some(task);
        }
    }

    fn read_vault_now(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        self.notes.loading = true;
        let read = cx.background_spawn(async move { read_vault(&root) });
        cx.spawn(async move |this, cx| {
            let read = read.await;
            this.update(cx, |shell, cx| shell.apply_vault(read, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn refresh_vault(&mut self, cx: &mut Context<Self>) {
        if let Some(root) = self.notes.root.clone() {
            self.read_vault_now(root, cx);
        }
    }

    fn apply_vault(
        &mut self,
        read: Result<VaultRead, krad_repo::RepoError>,
        cx: &mut Context<Self>,
    ) {
        self.notes.loading = false;
        match read {
            Ok(read) => {
                if self.notes.entries != read.entries || self.notes.index != read.index {
                    self.notes.entries = read.entries;
                    self.notes.index = read.index;
                    self.notes.graph.forget();
                    cx.notify();
                }
            }
            Err(error) => self.push_toast(error.to_string()),
        }
    }

    pub(super) fn open_vault_note(
        &mut self,
        path: &str,
        opening: FileOpening,
        cx: &mut Context<Self>,
    ) {
        self.notes.selected = Some(path.to_owned());
        for (folder, _) in path.match_indices('/') {
            self.notes.expanded.insert(path[..folder].to_owned());
        }
        self.edit_note_tabs(cx, |tabs| tabs.open(path, opening));
        if let Some(editor) = self.notes.editors.get_mut(path) {
            editor.focus_pending = true;
        }
    }

    pub(in crate::environment_shell) fn run_note_action(
        &mut self,
        action: NoteAction,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.notes.root.clone() else {
            return;
        };
        match action {
            NoteAction::NewNote(folder) => {
                let made =
                    cx.background_spawn(async move { create_note(&root, &folder, NEW_NOTE_TITLE) });
                cx.spawn(async move |this, cx| {
                    let made = made.await;
                    this.update(cx, |shell, cx| match made {
                        Ok(path) => {
                            shell.refresh_vault(cx);
                            shell.open_vault_note(&path, FileOpening::Pinned, cx);
                        }
                        Err(error) => shell.push_toast(error.to_string()),
                    })
                    .unwrap_or_default();
                })
                .detach();
            }
            NoteAction::NewFolder(folder) => {
                let made = cx.background_spawn(async move {
                    create_vault_folder(&root, &folder, NEW_FOLDER_NAME)
                });
                cx.spawn(async move |this, cx| {
                    let made = made.await;
                    this.update(cx, |shell, cx| match made {
                        Ok(path) => {
                            shell.notes.expanded.insert(path);
                            shell.refresh_vault(cx);
                        }
                        Err(error) => shell.push_toast(error.to_string()),
                    })
                    .unwrap_or_default();
                })
                .detach();
            }
            NoteAction::Open(path) => self.open_vault_note(&path, FileOpening::Pinned, cx),
            NoteAction::NewNamed(title) => {
                let made = cx.background_spawn(async move { create_note(&root, "", &title) });
                cx.spawn(async move |this, cx| {
                    let made = made.await;
                    this.update(cx, |shell, cx| match made {
                        Ok(path) => {
                            shell.refresh_vault(cx);
                            shell.open_vault_note(&path, FileOpening::Pinned, cx);
                        }
                        Err(error) => shell.push_toast(error.to_string()),
                    })
                    .unwrap_or_default();
                })
                .detach();
            }
        }
    }

    pub(super) fn relocate_notes(
        &mut self,
        from: &Path,
        to: Option<&Path>,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.notes.root.as_ref().map(|root| root.join(VAULT_DIR)) else {
            return;
        };
        let inside = |path: &Path| {
            path.strip_prefix(&vault)
                .ok()
                .map(|rest| rest.to_string_lossy().replace('\\', "/"))
        };
        let Some(from) = inside(from) else {
            return;
        };
        let to = to.and_then(inside);
        self.notes
            .editors
            .retain(|path, _| path != &from && !path.starts_with(&format!("{from}/")));
        self.edit_note_tabs(cx, |tabs| match &to {
            Some(to) => tabs.rename(&from, to),
            None => tabs.remove(&from),
        });
        self.refresh_vault(cx);
    }

    pub(in crate::environment_shell) fn notes_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_vault(cx);
        let pal = palette(cx);
        if self.notes.root.is_none() {
            return card(pal, cx)
                .size_full()
                .child(EmptyState::new(
                    IconName::Book,
                    NO_GROUP_TITLE,
                    NO_GROUP_TEXT,
                ))
                .into_any_element();
        }
        let sidebar = self.notes_sidebar(self.side_panel_width(), cx);
        let main = card(pal, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(self.note_column(cx));
        div()
            .flex()
            .flex_row()
            .size_full()
            .min_w_0()
            .min_h_0()
            .gap(px(WINDOW_INSET))
            .child(main)
            .children(sidebar)
            .into_any_element()
    }

    fn note_column(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let active = self.note_tabs().and_then(|tabs| tabs.active_tab().cloned());
        let body = match active {
            _ if self.notes.graph.showing => self.vault_graph(cx),
            Some(tab) => self.note_body(&tab.path, tab.mode, cx),
            None => div()
                .flex_1()
                .child(
                    EmptyState::new(IconName::Note, NO_NOTE_TITLE, NO_NOTE_TEXT).action(
                        Button::new(BTN_EMPTY_NEW_NOTE)
                            .icon(IconName::Add)
                            .label(NEW_NOTE_LABEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.run_note_action(NoteAction::NewNote(String::new()), cx);
                            })),
                    ),
                )
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(body)
            .into_any_element()
    }

    fn note_body(&mut self, path: &str, mode: NoteMode, cx: &mut Context<Self>) -> AnyElement {
        let content = match mode {
            NoteMode::Source => self.note_source(path, cx),
            NoteMode::Reading => self.note_reading(path, cx),
            NoteMode::Split => div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(self.note_source(path, cx)),
                )
                .child(div().w(px(SPLIT_RULE_W)).h_full().bg(palette(cx).divider))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(self.note_reading(path, cx)),
                )
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(content)
            .into_any_element()
    }
}
