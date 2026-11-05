use gpui_kit::{AppContext as _, Context, PromptButton, PromptLevel, Window};
use krad_domain::{EnvironmentName, TerminalId};
use krad_host::HostLink;
use std::path::PathBuf;
use std::time::Duration;

use super::super::disk_text::{disk_change, modified_at, DiskChange};
use super::super::{EnvironmentShell, LiveEnv};
use super::{DiskWatch, FileEditor, Load};
use crate::actions::SaveFile;

const CANCEL_LABEL: &str = "Cancel";
const DISCARD_LABEL: &str = "Close";
const SAVE_FAILED_TEXT: &str = "Couldn't save";
const UNSAVED_DETAIL: &str = "Your changes to this file will be lost.";
const UNSAVED_TITLE: &str = "Close without saving?";
const DISK_CHECK_EVERY: Duration = Duration::from_secs(2);

impl EnvironmentShell {
    pub(super) fn check_disk(
        link: HostLink,
        env: EnvironmentName,
        id: TerminalId,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let timer = cx.background_executor().timer(DISK_CHECK_EVERY);
        let (statted, watched) = (link.clone(), path.clone());
        let stat = cx.background_spawn(async move {
            timer.await;
            modified_at(&statted, &watched)
        });
        cx.spawn(async move |this, cx| {
            let disk = stat.await;
            this.update(cx, |shell, cx| {
                let shown = env == shell.active
                    && shell
                        .envs
                        .get(&env)
                        .and_then(LiveEnv::shown_file)
                        .is_some_and(|(shown, _)| shown == id);
                let Some(editor) = shell.editor_mut(&env, id) else {
                    return;
                };
                editor.disk = DiskWatch::Idle;
                match disk_change(editor.modified, disk, editor.dirty) {
                    DiskChange::Unchanged if shown => {
                        editor.disk = DiskWatch::Checking;
                        Self::check_disk(link, env, id, path, cx);
                        return;
                    }
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

    pub(super) fn on_save_file(&mut self, _: &SaveFile, _: &mut Window, cx: &mut Context<Self>) {
        self.save_shown_file(cx);
    }

    pub(super) fn save_shown_file(&mut self, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some((id, path)) = self.envs.get(&env).and_then(LiveEnv::shown_file) else {
            return;
        };
        let Some((state, edits)) = self
            .editor_mut(&env, id)
            .and_then(|editor| editor.state.clone().map(|state| (state, editor.edits)))
        else {
            return;
        };
        let text = state.read(cx).value().to_string();
        let saved = self.active_document(&path);
        let link = self.active_link();
        let writing = cx.background_spawn(async move {
            link.write(&path, text.as_bytes())
                .map(|()| modified_at(&link, &path))
        });
        cx.spawn(async move |this, cx| {
            let written = writing.await;
            this.update(cx, |shell, cx| {
                match written {
                    Ok(modified) => {
                        shell.document_saved(&saved);
                        if let Some(editor) = shell.editor_mut(&env, id) {
                            editor.saved(edits);
                            if editor.disk == DiskWatch::Conflict {
                                editor.disk = DiskWatch::Idle;
                            }
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

    pub(super) fn resolve_conflict(
        &mut self,
        id: TerminalId,
        reload: bool,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        if let Some(editor) = self.editor_mut(&env, id) {
            editor.disk = DiskWatch::Idle;
            if reload {
                editor.load = Load::Pending;
            }
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn request_close_tab(
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
            self.close_tab_asking(id, window, cx);
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
}
