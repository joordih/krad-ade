use gpui_kit::component::input::{Editor, EditorState, InputEvent};
use gpui_kit::{
    div, px, relative, rgb, AnyElement, AppContext as _, Context, Entity, ExternalPaths,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Styled, Subscription, Task,
    Window,
};
use krad_repo::write_note_text;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use super::super::disk_text::{
    disk_change, modified_at, read_file, DiskChange, ReadError, ReadFile,
};
mod attachments;

use super::super::EnvironmentShell;
use super::format_menu::{note_editor_menu, FormatNote};
use crate::components::{Button, ButtonVariant, EmptyState, IconName};
use crate::theme::{palette, LINE_SM, SPACE_1, SPACE_2, SPACE_3, TEXT_SM, UI_FONT_FAMILY};

const MARKDOWN: &str = "markdown";
const AUTOSAVE_AFTER: Duration = Duration::from_millis(700);
const DISK_CHECK_EVERY: Duration = Duration::from_secs(2);
const CHANGED_ON_DISK_TEXT: &str = "This note changed on disk.";
const BTN_RELOAD: &str = "note-reload";
const RELOAD_LABEL: &str = "Reload";
const BTN_KEEP: &str = "note-keep";
const KEEP_LABEL: &str = "Keep mine";
const OPEN_FAILED_TITLE: &str = "Can't open this note";
const PASTE_KEY: &str = "v";
const EDITOR_PAD_X: f32 = 32.0;
const EDITOR_PAD_TOP: f32 = 32.0;
const WRITING_W: f32 = 720.0;
const WRITING_TEXT: f32 = 16.0;
const WRITING_LINE_HEIGHT: f32 = 1.65;

pub(super) enum Load {
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

pub(in crate::environment_shell) struct NoteEditor {
    file: PathBuf,
    pub(super) state: Option<Entity<EditorState>>,
    pub(super) load: Load,
    dirty: bool,
    modified: Option<SystemTime>,
    pub(super) focus_pending: bool,
    disk: DiskWatch,
    changes: Option<Subscription>,
    save: Option<Task<()>>,
}

impl NoteEditor {
    fn new(file: PathBuf) -> Self {
        Self {
            file,
            state: None,
            load: Load::Pending,
            dirty: false,
            modified: None,
            focus_pending: true,
            disk: DiskWatch::Idle,
            changes: None,
            save: None,
        }
    }

    pub(super) const fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(super) fn text(&self, cx: &gpui_kit::App) -> Option<String> {
        self.state
            .as_ref()
            .map(|state| state.read(cx).value().to_string())
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn sync_notes_view(
        &mut self,
        view: krad_domain::EnvironmentView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if view == krad_domain::EnvironmentView::Notes {
            self.sync_note_editor(window, cx);
            self.reveal_pending_heading(cx);
            self.place_pending_line(window, cx);
        }
    }

    fn sync_note_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open: Vec<String> = self
            .note_tabs()
            .map_or_default(|tabs| tabs.tabs.iter().map(|tab| tab.path.clone()).collect());
        self.notes
            .editors
            .retain(|path, editor| open.contains(path) || editor.dirty);
        let Some(path) = self
            .note_tabs()
            .and_then(|tabs| tabs.active_tab())
            .map(|tab| tab.path.clone())
        else {
            return;
        };
        let Some(file) = self.vault_file(&path) else {
            return;
        };
        let editor = self
            .notes
            .editors
            .entry(path.clone())
            .or_insert_with(|| NoteEditor::new(file.clone()));
        match std::mem::replace(&mut editor.load, Load::Ready) {
            Load::Pending => {
                editor.load = Load::Reading;
                Self::read_note_file(path, file, cx);
                return;
            }
            Load::Read(read) => self.fill_note_editor(&path, read, window, cx),
            other => editor.load = other,
        }
        let Some(editor) = self.notes.editors.get_mut(&path) else {
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
            Self::check_note_disk(path, file, cx);
        }
    }

    fn read_note_file(path: String, file: PathBuf, cx: &mut Context<Self>) {
        let reading = cx.background_spawn(async move { read_file(&file) });
        cx.spawn(async move |this, cx| {
            let read = reading.await;
            this.update(cx, |shell, cx| {
                if let Some(editor) = shell.notes.editors.get_mut(&path) {
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

    fn fill_note_editor(
        &mut self,
        path: &str,
        read: ReadFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.notes.editors.get_mut(path) else {
            return;
        };
        editor.modified = read.modified;
        editor.dirty = false;
        editor.disk = DiskWatch::Idle;
        if let Some(state) = editor.state.clone() {
            state.update(cx, |state, cx| state.set_value(read.text, window, cx));
            return;
        }
        let state = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(MARKDOWN)
                .soft_wrap(true)
                .line_number(false)
                .folding(false)
                .default_value(read.text)
        });
        let edited = path.to_owned();
        let changes = cx.subscribe(&state, move |this: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.note_edited(&edited, cx);
            }
        });
        if let Some(editor) = self.notes.editors.get_mut(path) {
            editor.state = Some(state);
            editor.changes = Some(changes);
        }
    }

    fn note_edited(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(editor) = self.notes.editors.get_mut(path) else {
            return;
        };
        if !editor.dirty {
            editor.dirty = true;
            let index = self
                .note_tabs()
                .and_then(|tabs| tabs.tabs.iter().position(|tab| tab.path == path));
            if let Some(index) = index {
                self.edit_note_tabs(cx, |tabs| tabs.pin(index));
            }
        }
        let saved = path.to_owned();
        let timer = cx.background_executor().timer(AUTOSAVE_AFTER);
        let task = cx.spawn(async move |this, cx| {
            timer.await;
            this.update(cx, |shell, cx| shell.save_note(&saved, cx))
                .unwrap_or_default();
        });
        if let Some(editor) = self.notes.editors.get_mut(path) {
            editor.save = Some(task);
        }
        cx.notify();
    }

    pub(super) fn save_note(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(root) = self.notes.root.clone() else {
            return;
        };
        let Some(editor) = self.notes.editors.get_mut(path) else {
            return;
        };
        if !editor.dirty {
            return;
        }
        let Some(text) = editor.text(cx) else {
            return;
        };
        editor.save = None;
        let (written, file, saved) = (path.to_owned(), editor.file.clone(), text.clone());
        let write = cx.background_spawn(async move {
            write_note_text(&root, &written, &saved).map(|()| modified_at(&file))
        });
        let path = path.to_owned();
        cx.spawn(async move |this, cx| {
            let result = write.await;
            this.update(cx, |shell, cx| {
                match result {
                    Ok(modified) => {
                        if let Some(editor) = shell.notes.editors.get_mut(&path) {
                            editor.modified = modified;
                            let unchanged = editor.text(cx).is_some_and(|now| now == text);
                            editor.dirty = !unchanged;
                        }
                        shell.refresh_vault(cx);
                    }
                    Err(error) => shell.push_toast(error.to_string()),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn check_note_disk(path: String, file: PathBuf, cx: &mut Context<Self>) {
        let timer = cx.background_executor().timer(DISK_CHECK_EVERY);
        let stat = cx.background_spawn(async move {
            timer.await;
            modified_at(&file)
        });
        cx.spawn(async move |this, cx| {
            let disk = stat.await;
            this.update(cx, |shell, cx| {
                let Some(editor) = shell.notes.editors.get_mut(&path) else {
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

    fn resolve_note_conflict(&mut self, path: &str, reload: bool, cx: &mut Context<Self>) {
        let Some(editor) = self.notes.editors.get_mut(path) else {
            return;
        };
        editor.disk = DiskWatch::Idle;
        if reload {
            editor.dirty = false;
            editor.load = Load::Pending;
        } else {
            self.save_note(path, cx);
        }
        cx.notify();
    }

    pub(super) fn note_source(&self, path: &str, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let Some(editor) = self.notes.editors.get(path) else {
            return div().flex_1().into_any_element();
        };
        if let Load::Failed(error) = &editor.load {
            return EmptyState::new(IconName::Note, OPEN_FAILED_TITLE, error.text().to_owned())
                .into_any_element();
        }
        let Some(state) = &editor.state else {
            return div().flex_1().into_any_element();
        };
        let conflict = (editor.disk == DiskWatch::Conflict).then(|| {
            let (reload, keep) = (path.to_owned(), path.to_owned());
            div()
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(SPACE_2))
                .px(px(SPACE_3))
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
                            this.resolve_note_conflict(&reload, true, cx);
                        })),
                )
                .child(
                    Button::new(BTN_KEEP)
                        .label(KEEP_LABEL)
                        .small()
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.resolve_note_conflict(&keep, false, cx);
                        })),
                )
        });
        let (dropped, pasted, formatted) = (path.to_owned(), path.to_owned(), path.to_owned());
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .children(conflict)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_center()
                    .flex_1()
                    .min_h_0()
                    .px(px(EDITOR_PAD_X))
                    .pt(px(EDITOR_PAD_TOP))
                    .on_action(cx.listener(
                        move |this: &mut Self, action: &FormatNote, window, cx| {
                            this.format_note(&formatted, action.0, window, cx);
                        },
                    ))
                    .on_drop(cx.listener(
                        move |this: &mut Self, files: &ExternalPaths, window, cx| {
                            this.attach_files(&dropped, files.paths().to_vec(), window, cx);
                        },
                    ))
                    .capture_key_down(cx.listener(
                        move |this: &mut Self, event: &KeyDownEvent, window, cx| {
                            let keystroke = &event.keystroke;
                            if keystroke.key == PASTE_KEY
                                && keystroke.modifiers.secondary()
                                && this.paste_image(&pasted, window, cx)
                            {
                                cx.stop_propagation();
                            }
                        },
                    ))
                    .child(
                        div().flex_1().max_w(px(WRITING_W)).h_full().child(
                            Editor::new(state)
                                .appearance(false)
                                .bordered(false)
                                .h_full()
                                .font_family(UI_FONT_FAMILY)
                                .text_size(px(WRITING_TEXT))
                                .line_height(relative(WRITING_LINE_HEIGHT))
                                .context_menu(|menu, _, _| note_editor_menu(menu)),
                        ),
                    ),
            )
            .into_any_element()
    }
}
