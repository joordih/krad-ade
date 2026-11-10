use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, IntoElement, ParentElement, PromptButton,
    PromptLevel, Styled, Window,
};
use krad_domain::{EnvironmentName, GitObjectId, TerminalId};
use krad_repo::{checkpoint_changes, restore_checkpoint, CheckpointChange};

use super::super::super::EnvironmentShell;
use super::{files_detail, surface_chat, RewindProgress, CANCEL_LABEL, KEPT_NOTE};
use crate::components::{Button, ButtonVariant};
use crate::theme::{Palette, LINE_XS, SPACE_2, TEXT_XS};

const UNDO_TITLE: &str = "Undo the file restore?";
const UNDO_DETAIL: &str = "These files go back to how they were just before the restore:";
const UNDO_LABEL: &str = "Undo";
const DISMISS_LABEL: &str = "Dismiss";
const UNDO_ANSWER: usize = 0;
const RESTORED_TEXT: &str = "Files restored to before the edited message.";
const PUT_BACK_TEXT: &str = "The rewind failed, so the restored files were put back.";
const NOTHING_TO_UNDO_TEXT: &str = "The files already match the moment before the restore.";
const UNDO_PREFIX: &str = "surface-files-undo";
const DISMISS_PREFIX: &str = "surface-files-dismiss";

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn files_rewind_settled(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        rewound: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = surface_chat(self, env, id) else {
            return;
        };
        let Some(files) = chat.files_undo.as_mut() else {
            return;
        };
        if files.rewind == RewindProgress::Settled {
            return;
        }
        if rewound {
            files.rewind = RewindProgress::Settled;
            return;
        }
        let undo = files.undo.clone();
        chat.files_undo = None;
        self.put_files_back(env, id, undo, Some(PUT_BACK_TEXT), cx);
    }

    fn put_files_back(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        undo: GitObjectId,
        notice: Option<&'static str>,
        cx: &mut Context<Self>,
    ) {
        let Some((link, root)) = self.group_folder(env, id) else {
            return;
        };
        let restored = cx.background_spawn(async move { restore_checkpoint(&link, &root, &undo) });
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let restored = restored.await;
            this.update(cx, |shell, cx| {
                let Some(chat) = surface_chat(shell, &env, id) else {
                    return;
                };
                let shown = match restored {
                    Ok(_) => notice.map(str::to_owned),
                    Err(error) => Some(error.to_string()),
                };
                if let Some(shown) = shown {
                    chat.problem = Some(shown);
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn undo_files_restore(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(undo) = surface_chat(self, env, id)
            .and_then(|chat| chat.files_undo.as_ref())
            .map(|files| files.undo.clone())
        else {
            return;
        };
        let Some((link, root)) = self.group_folder(env, id) else {
            return;
        };
        let target = undo.clone();
        let listed = cx.background_spawn(async move { checkpoint_changes(&link, &root, &target) });
        let env = env.clone();
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await;
            this.update_in(cx, |shell, window, cx| {
                match listed {
                    Ok(changes) if changes.is_empty() => {
                        if let Some(chat) = surface_chat(shell, &env, id) {
                            chat.files_undo = None;
                            chat.problem = Some(NOTHING_TO_UNDO_TEXT.to_owned());
                        }
                    }
                    Ok(changes) => Self::confirm_undo(&env, id, undo, &changes, window, cx),
                    Err(error) => {
                        if let Some(chat) = surface_chat(shell, &env, id) {
                            chat.problem = Some(error.to_string());
                        }
                    }
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn confirm_undo(
        env: &EnvironmentName,
        id: TerminalId,
        undo: GitObjectId,
        changes: &[CheckpointChange],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let detail = files_detail(UNDO_DETAIL, changes, Some(KEPT_NOTE));
        let answer = window.prompt(
            PromptLevel::Warning,
            UNDO_TITLE,
            Some(&detail),
            &[
                PromptButton::ok(UNDO_LABEL),
                PromptButton::cancel(CANCEL_LABEL),
            ],
            cx,
        );
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            if !matches!(answer.await, Ok(UNDO_ANSWER)) {
                return;
            }
            this.update(cx, |shell, cx| {
                if let Some(chat) = surface_chat(shell, &env, id) {
                    chat.files_undo = None;
                }
                shell.put_files_back(&env, id, undo, None, cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell::surface) fn files_restored_row(
        id: TerminalId,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let raw = id.as_u64();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .text_color(rgb(pal.text_muted))
            .child(RESTORED_TEXT)
            .child(
                Button::new((UNDO_PREFIX, raw))
                    .label(UNDO_LABEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        let env = this.active.clone();
                        this.undo_files_restore(&env, id, window, cx);
                    })),
            )
            .child(
                Button::new((DISMISS_PREFIX, raw))
                    .label(DISMISS_LABEL)
                    .small()
                    .variant(ButtonVariant::Ghost)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        let env = this.active.clone();
                        if let Some(chat) = surface_chat(this, &env, id) {
                            chat.files_undo = None;
                        }
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
