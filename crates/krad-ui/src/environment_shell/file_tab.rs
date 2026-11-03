mod disk;
mod minimap;
mod problem_lens;
mod problem_marks;

use gpui_kit::component::input::{Editor, EditorState, InputEvent};
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Styled, Subscription, Window,
};
use krad_domain::{EnvironmentName, FileOpening, FileView, TerminalId};
use krad_host::HostLink;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::disk_text::{read_file, ReadError, ReadFile};
use super::language_servers::Document;
use super::{EnvironmentShell, LiveEnv};
use crate::actions::FILE_TAB_KEY_CONTEXT;
use crate::components::{Button, ButtonVariant, EmptyState, IconName};
use crate::highlight::Syntax;
use crate::language::editor_id;
use crate::theme::{palette, LINE_SM, SPACE_1, SPACE_1_5, SPACE_2, TEXT_SM};
use minimap::{minimap, new_minimap, Minimap};
use problem_lens::problem_lens;
use problem_marks::problem_marks;

const OPEN_FAILED_TITLE: &str = "Can't open this file";
const CHANGED_ON_DISK_TEXT: &str = "This file changed on disk.";
const BTN_RELOAD: &str = "file-tab-reload";
const RELOAD_LABEL: &str = "Reload";
const BTN_KEEP: &str = "file-tab-keep";
const KEEP_LABEL: &str = "Keep mine";

enum Load {
    Pending,
    Reading,
    Read(ReadFile),
    Ready,
    Failed(ReadError),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DiskWatch {
    Idle,
    Checking,
    Conflict,
}

pub(super) struct FileEditor {
    pub(super) path: PathBuf,
    pub(super) state: Option<Entity<EditorState>>,
    minimap: Option<Entity<Minimap>>,
    load: Load,
    dirty: bool,
    edits: EditCount,
    disk: DiskWatch,
    modified: Option<SystemTime>,
    focus_pending: bool,
    changes: Option<Subscription>,
}

impl FileEditor {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            state: None,
            minimap: None,
            load: Load::Pending,
            dirty: false,
            edits: EditCount::default(),
            disk: DiskWatch::Idle,
            modified: None,
            focus_pending: true,
            changes: None,
        }
    }

    pub(super) const fn is_dirty(&self) -> bool {
        self.dirty
    }

    const fn edited(&mut self) {
        self.dirty = true;
        self.edits = self.edits.next();
    }

    const fn saved(&mut self, at: EditCount) {
        if self.edits.0 == at.0 {
            self.dirty = false;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct EditCount(u64);

impl EditCount {
    const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

impl LiveEnv {
    pub(super) fn shown_file(&self) -> Option<(TerminalId, PathBuf)> {
        let id = self.shown_tab()?;
        let file = self
            .data
            .open_file_of(id)
            .filter(|file| file.view == FileView::Edit)?;
        Some((id, file.path.clone()))
    }
}

impl EnvironmentShell {
    pub(super) fn open_file_tab(
        &mut self,
        path: &Path,
        opening: FileOpening,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let id = live.data.open_file(live.selected_group, path, opening);
        live.focused = Some(id);
        if let Some(editor) = live.editors.get_mut(&id) {
            editor.focus_pending = true;
        }
        cx.notify();
        self.store();
    }

    pub(super) fn shown_file_path(&self) -> Option<PathBuf> {
        self.envs
            .get(&self.active)
            .and_then(LiveEnv::shown_file)
            .map(|(_, path)| path)
    }

    pub(super) fn pin_file_tab(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        if let Some(live) = self.envs.get_mut(&self.active) {
            live.data.pin_file(id);
        }
        cx.notify();
        self.store();
    }

    pub(super) fn show_file_tab(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        self.focus_terminal(id);
        if let Some(editor) = self
            .envs
            .get_mut(&self.active)
            .and_then(|live| live.editors.get_mut(&id))
        {
            editor.focus_pending = true;
        }
        cx.notify();
    }

    pub(super) fn sync_file_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let files: Vec<TerminalId> = live
            .data
            .nodes
            .iter()
            .filter(|node| node.file.is_some())
            .map(|node| node.id)
            .collect();
        live.editors.retain(|id, _| files.contains(id));
        self.close_unseen_documents();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let Some((id, path)) = live.shown_file() else {
            return;
        };
        let link = live.link.clone();
        let editor = live
            .editors
            .entry(id)
            .or_insert_with(|| FileEditor::new(path.clone()));
        if editor.path != path {
            *editor = FileEditor::new(path.clone());
        }
        match std::mem::replace(&mut editor.load, Load::Ready) {
            Load::Pending => {
                editor.load = Load::Reading;
                Self::read_into_tab(link, env, id, path, cx);
                return;
            }
            Load::Read(file) => self.fill_editor(&env, id, file, window, cx),
            other => editor.load = other,
        }
        let Some(editor) = self.editor_mut(&env, id) else {
            return;
        };
        if editor.focus_pending {
            if let Some(state) = editor.state.clone() {
                editor.focus_pending = false;
                state.update(cx, |state, cx| state.focus(window, cx));
            }
        }
        let state = editor.state.clone();
        let cursor = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.take_pending_cursor(id));
        if let (Some(state), Some(cursor)) = (state, cursor) {
            state.update(cx, |state, cx| {
                state.set_cursor_position(cursor, window, cx);
            });
        }
        let Some(editor) = self.editor_mut(&env, id) else {
            return;
        };
        if matches!(editor.load, Load::Ready) && editor.disk == DiskWatch::Idle {
            editor.disk = DiskWatch::Checking;
            Self::check_disk(link, env, id, path, cx);
        }
    }

    fn editor_mut(&mut self, env: &EnvironmentName, id: TerminalId) -> Option<&mut FileEditor> {
        self.envs.get_mut(env)?.editors.get_mut(&id)
    }

    fn read_into_tab(
        link: HostLink,
        env: EnvironmentName,
        id: TerminalId,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let reading = cx.background_spawn(async move { read_file(&link, &path) });
        cx.spawn(async move |this, cx| {
            let read = reading.await;
            this.update(cx, |shell, cx| {
                if let Some(editor) = shell.editor_mut(&env, id) {
                    editor.load = match read {
                        Ok(file) => Load::Read(file),
                        Err(error) => Load::Failed(error),
                    };
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn fill_editor(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        file: ReadFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = self
            .envs
            .get(env)
            .and_then(|live| live.editors.get(&id))
            .and_then(|editor| self.language_of(&editor.path));
        let syntax = Syntax::from(language.as_ref());
        let grammar = language
            .as_ref()
            .and_then(|language| editor_id(language).map(str::to_owned));
        let host = self
            .envs
            .get(env)
            .map_or_else(krad_domain::Host::default, |live| live.data.host.clone());
        let Some(editor) = self.editor_mut(env, id) else {
            return;
        };
        editor.modified = file.modified;
        editor.dirty = false;
        editor.disk = DiskWatch::Idle;
        if let Some(state) = editor.state.clone() {
            if let Some(minimap) = editor.minimap.clone() {
                let text = file.text.clone();
                minimap.update(cx, |minimap, cx| minimap.rebuild(text, syntax, cx));
            }
            state.update(cx, |state, cx| state.set_value(file.text, window, cx));
            return;
        }
        let shown_minimap = new_minimap(&file.text, syntax, cx);
        let state = cx.new(|cx| {
            let state = EditorState::new(window, cx);
            let state = match grammar {
                Some(language) => state.language(language),
                None => state,
            };
            state.default_value(file.text)
        });
        let edited_env = env.clone();
        let edited = Document::new(&host, &editor.path);
        let rebuilt = shown_minimap.clone();
        let changes = cx.subscribe(
            &state,
            move |this: &mut Self, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = state.read(cx).value().to_string();
                    rebuilt.update(cx, |minimap, cx| minimap.rebuild(text.clone(), syntax, cx));
                    this.mark_edited(&edited_env, id, cx);
                    this.document_edited(&edited, text, cx);
                    this.show_diagnostics(&edited, cx);
                }
            },
        );
        let path = editor.path.clone();
        if let Some(editor) = self.editor_mut(env, id) {
            editor.state = Some(state.clone());
            editor.minimap = Some(shown_minimap);
            editor.changes = Some(changes);
        }
        self.attach_language_server(id, &path, &state, cx);
    }

    fn mark_edited(&mut self, env: &EnvironmentName, id: TerminalId, cx: &mut Context<Self>) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        if let Some(editor) = live.editors.get_mut(&id) {
            editor.edited();
        }
        let pinned = live.data.open_file_of(id).is_some_and(|file| file.pinned);
        if !pinned {
            live.data.pin_file(id);
            self.store();
        }
        cx.notify();
    }

    pub(super) fn file_tab_body(
        &self,
        live: &LiveEnv,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let Some(editor) = live.editors.get(&id) else {
            return div().flex_1().into_any_element();
        };
        if let Load::Failed(error) = &editor.load {
            return EmptyState::new(IconName::File, OPEN_FAILED_TITLE, error.text().to_owned())
                .into_any_element();
        }
        let Some(state) = &editor.state else {
            return div().flex_1().into_any_element();
        };
        let editor_settings = self.settings.editor;
        let conflict = (editor.disk == DiskWatch::Conflict).then(|| {
            div()
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(SPACE_2))
                .px(px(SPACE_2))
                .py(px(SPACE_1))
                .bg(pal.bg_hover)
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(pal.warning_fg))
                .child(div().flex_1().child(CHANGED_ON_DISK_TEXT))
                .child(
                    Button::new(BTN_RELOAD)
                        .label(RELOAD_LABEL)
                        .small()
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.resolve_conflict(id, true, cx);
                        })),
                )
                .child(
                    Button::new(BTN_KEEP)
                        .label(KEEP_LABEL)
                        .small()
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.resolve_conflict(id, false, cx);
                        })),
                )
        });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .key_context(FILE_TAB_KEY_CONTEXT)
            .on_action(cx.listener(Self::on_save_file))
            .on_action(cx.listener(Self::on_find_references))
            .on_action(cx.listener(Self::on_accept_completion))
            .on_action(cx.listener(Self::on_duplicate_line))
            .on_action(cx.listener(Self::on_delete_line))
            .on_action(cx.listener(Self::on_move_line_up))
            .on_action(cx.listener(Self::on_move_line_down))
            .on_action(cx.listener(Self::on_toggle_line_comment))
            .on_action(cx.listener(Self::on_next_problem))
            .on_action(cx.listener(Self::on_previous_problem))
            .on_action(cx.listener(Self::on_rename_symbol))
            .on_action(cx.listener(Self::on_reformat))
            .children(self.language_notice(live, id, &editor.path, cx))
            .children(conflict)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .pl(px(SPACE_1_5))
                            .child(
                                Editor::new(state)
                                    .bordered(false)
                                    .readonly(self.is_library_document(&Document::new(
                                        &live.data.host,
                                        &editor.path,
                                    )))
                                    .h_full(),
                            )
                            .child(problem_lens(state.clone(), editor_settings.problem_lens))
                            .child(problem_marks(state.clone())),
                    )
                    .children(
                        editor
                            .minimap
                            .clone()
                            .filter(|_| editor_settings.minimap.is_shown())
                            .map(|shown| minimap(shown, state.clone())),
                    ),
            )
            .children(self.toolchain_status(live, &editor.path, cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_only_cleans_the_edits_it_wrote() {
        let mut editor = FileEditor::new(PathBuf::from("/srv/app/main.py"));
        editor.edited();
        let saving = editor.edits;
        editor.edited();
        editor.saved(saving);
        assert!(editor.is_dirty());
        let saving = editor.edits;
        editor.saved(saving);
        assert!(!editor.is_dirty());
    }
}
