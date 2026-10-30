use gpui_kit::component::input::{Editor, EditorState, InputEvent};
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, PromptButton, PromptLevel, Styled, Subscription, Window,
};
use krad_domain::{EnvironmentName, FileOpening, FileView, TerminalId};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::disk_text::{disk_change, modified_at, read_file, DiskChange, ReadError, ReadFile};
use super::{EnvironmentShell, LiveEnv};
use crate::actions::{SaveFile, FILE_TAB_KEY_CONTEXT};
use crate::components::{Button, ButtonVariant, EmptyState, IconName};
use crate::language::Language;
use crate::theme::{palette, LINE_SM, SPACE_1, SPACE_1_5, SPACE_2, TEXT_SM};

const DISK_CHECK_EVERY: Duration = Duration::from_secs(2);
const OPEN_FAILED_TITLE: &str = "Can't open this file";
const CHANGED_ON_DISK_TEXT: &str = "This file changed on disk.";
const BTN_RELOAD: &str = "file-tab-reload";
const RELOAD_LABEL: &str = "Reload";
const BTN_KEEP: &str = "file-tab-keep";
const KEEP_LABEL: &str = "Keep mine";
const SAVE_FAILED_TEXT: &str = "Couldn't save";
const UNSAVED_TITLE: &str = "Close without saving?";
const UNSAVED_DETAIL: &str = "Your changes to this file will be lost.";
const DISCARD_LABEL: &str = "Close";
const CANCEL_LABEL: &str = "Cancel";

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
    path: PathBuf,
    state: Option<Entity<EditorState>>,
    load: Load,
    dirty: bool,
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
            load: Load::Pending,
            dirty: false,
            disk: DiskWatch::Idle,
            modified: None,
            focus_pending: true,
            changes: None,
        }
    }

    pub(super) const fn is_dirty(&self) -> bool {
        self.dirty
    }
}

impl LiveEnv {
    fn shown_file(&self) -> Option<(TerminalId, PathBuf)> {
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
        let Some((id, path)) = live.shown_file() else {
            return;
        };
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
                Self::read_into_tab(env, id, path, cx);
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
        if matches!(editor.load, Load::Ready) && editor.disk == DiskWatch::Idle {
            editor.disk = DiskWatch::Checking;
            Self::check_disk(env, id, path, cx);
        }
    }

    fn editor_mut(&mut self, env: &EnvironmentName, id: TerminalId) -> Option<&mut FileEditor> {
        self.envs.get_mut(env)?.editors.get_mut(&id)
    }

    fn read_into_tab(env: EnvironmentName, id: TerminalId, path: PathBuf, cx: &mut Context<Self>) {
        let reading = cx.background_spawn(async move { read_file(&path) });
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
        let Some(editor) = self.editor_mut(env, id) else {
            return;
        };
        editor.modified = file.modified;
        editor.dirty = false;
        editor.disk = DiskWatch::Idle;
        if let Some(state) = editor.state.clone() {
            state.update(cx, |state, cx| state.set_value(file.text, window, cx));
            return;
        }
        let language = Language::from_path(&editor.path).and_then(Language::editor_id);
        let state = cx.new(|cx| {
            let state = EditorState::new(window, cx);
            let state = match language {
                Some(language) => state.language(language),
                None => state,
            };
            state.default_value(file.text)
        });
        let edited_env = env.clone();
        let changes = cx.subscribe(&state, move |this: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.mark_edited(&edited_env, id, cx);
            }
        });
        if let Some(editor) = self.editor_mut(env, id) {
            editor.state = Some(state);
            editor.changes = Some(changes);
        }
    }

    fn mark_edited(&mut self, env: &EnvironmentName, id: TerminalId, cx: &mut Context<Self>) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        if let Some(editor) = live.editors.get_mut(&id) {
            editor.dirty = true;
        }
        let pinned = live.data.open_file_of(id).is_some_and(|file| file.pinned);
        if !pinned {
            live.data.pin_file(id);
            self.store();
        }
        cx.notify();
    }

    fn check_disk(env: EnvironmentName, id: TerminalId, path: PathBuf, cx: &mut Context<Self>) {
        let timer = cx.background_executor().timer(DISK_CHECK_EVERY);
        let stat = cx.background_spawn(async move {
            timer.await;
            modified_at(&path)
        });
        cx.spawn(async move |this, cx| {
            let disk = stat.await;
            this.update(cx, |shell, cx| {
                let Some(editor) = shell.editor_mut(&env, id) else {
                    return;
                };
                editor.disk = DiskWatch::Idle;
                match disk_change(editor.modified, disk, editor.dirty) {
                    DiskChange::Unchanged => {}
                    DiskChange::Reload => editor.load = Load::Pending,
                    DiskChange::Conflict => {
                        editor.disk = DiskWatch::Conflict;
                        editor.modified = disk;
                    }
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn on_save_file(&mut self, _: &SaveFile, _: &mut Window, cx: &mut Context<Self>) {
        self.save_shown_file(cx);
    }

    fn save_shown_file(&mut self, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some((id, path)) = self.envs.get(&env).and_then(LiveEnv::shown_file) else {
            return;
        };
        let Some(state) = self
            .editor_mut(&env, id)
            .and_then(|editor| editor.state.clone())
        else {
            return;
        };
        let text = state.read(cx).value().to_string();
        let writing =
            cx.background_spawn(
                async move { std::fs::write(&path, text).map(|()| modified_at(&path)) },
            );
        cx.spawn(async move |this, cx| {
            let written = writing.await;
            this.update(cx, |shell, cx| {
                match written {
                    Ok(modified) => {
                        if let Some(editor) = shell.editor_mut(&env, id) {
                            editor.dirty = false;
                            editor.disk = DiskWatch::Idle;
                            editor.modified = modified;
                        }
                    }
                    Err(error) => shell.push_toast(format!("{SAVE_FAILED_TEXT}: {error}")),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn resolve_conflict(&mut self, id: TerminalId, reload: bool, cx: &mut Context<Self>) {
        let env = self.active.clone();
        if let Some(editor) = self.editor_mut(&env, id) {
            editor.disk = DiskWatch::Idle;
            if reload {
                editor.load = Load::Pending;
            }
        }
        cx.notify();
    }

    pub(super) fn request_close_tab(
        &mut self,
        id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dirty = self
            .envs
            .get(&self.active)
            .and_then(|live| live.editors.get(&id))
            .is_some_and(FileEditor::is_dirty);
        if !dirty {
            self.close_terminal(id, cx);
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            UNSAVED_TITLE,
            Some(UNSAVED_DETAIL),
            &[
                PromptButton::ok(DISCARD_LABEL),
                PromptButton::cancel(CANCEL_LABEL),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if matches!(answer.await, Ok(0)) {
                this.update(cx, |this, cx| this.close_terminal(id, cx))
                    .unwrap_or_default();
            }
        })
        .detach();
    }

    pub(super) fn file_tab_body(
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
            .children(conflict)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .pl(px(SPACE_1_5))
                    .child(Editor::new(state).bordered(false).h_full()),
            )
            .into_any_element()
    }
}
