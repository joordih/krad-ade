use gpui_kit::{AppContext as _, Context, PromptButton, PromptLevel, Window};
use krad_domain::{EnvironmentName, GitObjectId, Prompt, TerminalId};
use krad_repo::{
    checkpoint_changes, restore_checkpoint, CheckpointChange, CheckpointChangeKind, RepoError,
    RestoredCheckpoint,
};

use super::super::EnvironmentShell;
use super::{Backend, SurfaceChat};
use krad_chat::RewindScope;

mod undo;

const REWIND_TITLE: &str = "Rewind the files too?";
const REWIND_DETAIL: &str = "Both choices remove the later messages. Restoring the files puts these back as they were before this message, including changes made by shell commands:";
const KEPT_NOTE: &str = "Your current files are kept, so the restore can be undone.";
const BOTH_LABEL: &str = "Conversation and files";
const CONVERSATION_LABEL: &str = "Conversation only";
const CANCEL_LABEL: &str = "Cancel";
const BOTH_ANSWER: usize = 0;
const CONVERSATION_ANSWER: usize = 1;
const MAX_LISTED_FILES: usize = 12;
const LIST_BULLET: &str = "• ";
const RESTORED_SUFFIX: &str = "";
const RECREATED_SUFFIX: &str = " (brought back)";
const REMOVED_SUFFIX: &str = " (deleted)";
const PARAGRAPH: &str = "\n\n";
const LINE_BREAK: &str = "\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RewindProgress {
    Waiting,
    Settled,
}

pub(super) struct FilesRewind {
    pub(super) index: usize,
    pub(super) prompt: Prompt,
    pub(super) checkpoint: GitObjectId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FilesUndo {
    undo: GitObjectId,
    rewind: RewindProgress,
}

const fn change_suffix(kind: CheckpointChangeKind) -> &'static str {
    match kind {
        CheckpointChangeKind::Reverted => RESTORED_SUFFIX,
        CheckpointChangeKind::Recreated => RECREATED_SUFFIX,
        CheckpointChangeKind::Removed => REMOVED_SUFFIX,
    }
}

fn files_detail(intro: &str, changes: &[CheckpointChange], note: Option<&str>) -> String {
    let listed = changes.iter().take(MAX_LISTED_FILES).map(|change| {
        format!(
            "{LIST_BULLET}{}{}",
            change.path.display(),
            change_suffix(change.kind)
        )
    });
    let hidden = changes.len().saturating_sub(MAX_LISTED_FILES);
    let more = (hidden > 0).then(|| format!("{LIST_BULLET}and {hidden} more"));
    let lines: Vec<String> = std::iter::once(intro.to_owned())
        .chain(listed)
        .chain(more)
        .collect();
    let detail = lines.join(LINE_BREAK);
    match note {
        Some(note) => [detail.as_str(), note].join(PARAGRAPH),
        None => detail,
    }
}

fn surface_chat<'a>(
    shell: &'a mut EnvironmentShell,
    env: &EnvironmentName,
    id: TerminalId,
) -> Option<&'a mut SurfaceChat> {
    shell.envs.get_mut(env)?.chats.get_mut(&id)
}

fn set_busy(shell: &mut EnvironmentShell, env: &EnvironmentName, id: TerminalId, busy: bool) {
    if let Some(Backend::Structured(structured)) =
        surface_chat(shell, env, id).map(|chat| &mut chat.backend)
    {
        structured.working = busy;
    }
}

impl EnvironmentShell {
    pub(super) fn rewind_with_files(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        rewind: FilesRewind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((link, root)) = self.group_folder(env, id) else {
            return self.rewind_conversation(env, id, rewind, cx);
        };
        set_busy(self, env, id, true);
        let target = rewind.checkpoint.clone();
        let listed = cx.background_spawn(async move { checkpoint_changes(&link, &root, &target) });
        let env = env.clone();
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await;
            this.update_in(cx, |shell, window, cx| {
                set_busy(shell, &env, id, false);
                let changes = listed.unwrap_or_else(|error| {
                    tracing::warn!(terminal = id.as_u64(), %error, "could not compare the turn's files");
                    Vec::new()
                });
                if changes.is_empty() {
                    shell.rewind_conversation(&env, id, rewind, cx);
                } else {
                    Self::ask_rewind_files(&env, id, rewind, &changes, window, cx);
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn rewind_conversation(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        rewind: FilesRewind,
        cx: &mut Context<Self>,
    ) {
        self.start_rewind(
            env,
            id,
            rewind.index,
            rewind.prompt,
            RewindScope::Conversation,
            cx,
        );
    }

    fn ask_rewind_files(
        env: &EnvironmentName,
        id: TerminalId,
        rewind: FilesRewind,
        changes: &[CheckpointChange],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let detail = files_detail(REWIND_DETAIL, changes, Some(KEPT_NOTE));
        let answer = window.prompt(
            PromptLevel::Warning,
            REWIND_TITLE,
            Some(&detail),
            &[
                PromptButton::ok(BOTH_LABEL),
                PromptButton::new(CONVERSATION_LABEL),
                PromptButton::cancel(CANCEL_LABEL),
            ],
            cx,
        );
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let answer = answer.await;
            this.update(cx, |shell, cx| {
                match answer {
                    Ok(BOTH_ANSWER) => shell.restore_then_rewind(&env, id, rewind, cx),
                    Ok(CONVERSATION_ANSWER) => shell.rewind_conversation(&env, id, rewind, cx),
                    _ => {}
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn restore_then_rewind(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        rewind: FilesRewind,
        cx: &mut Context<Self>,
    ) {
        let Some((link, root)) = self.group_folder(env, id) else {
            return;
        };
        set_busy(self, env, id, true);
        let target = rewind.checkpoint.clone();
        let restored =
            cx.background_spawn(async move { restore_checkpoint(&link, &root, &target) });
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let restored = restored.await;
            this.update(cx, |shell, cx| {
                set_busy(shell, &env, id, false);
                shell.files_restored(&env, id, rewind, restored, cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn files_restored(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        rewind: FilesRewind,
        restored: Result<RestoredCheckpoint, RepoError>,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = surface_chat(self, env, id) else {
            return;
        };
        match restored {
            Ok(restored) => {
                chat.files_undo = Some(FilesUndo {
                    undo: restored.undo,
                    rewind: RewindProgress::Waiting,
                });
                self.rewind_conversation(env, id, rewind, cx);
            }
            Err(error) => chat.problem = Some(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn change(path: &str, kind: CheckpointChangeKind) -> CheckpointChange {
        CheckpointChange {
            path: PathBuf::from(path),
            kind,
        }
    }

    #[test]
    fn the_dialog_lists_each_file_and_what_happens_to_it() {
        let changes = [
            change("src/a.rs", CheckpointChangeKind::Reverted),
            change("notes.txt", CheckpointChangeKind::Recreated),
            change("new.rs", CheckpointChangeKind::Removed),
        ];
        assert_eq!(
            files_detail("Files:", &changes, Some("Kept.")),
            "Files:\n• src/a.rs\n• notes.txt (brought back)\n• new.rs (deleted)\n\nKept."
        );
    }

    #[test]
    fn a_long_list_is_cut_with_a_count_of_the_rest() {
        let changes: Vec<CheckpointChange> = (0..MAX_LISTED_FILES + 3)
            .map(|index| change(&format!("f{index}"), CheckpointChangeKind::Reverted))
            .collect();
        let detail = files_detail("Files:", &changes, None);
        assert_eq!(detail.lines().count(), 1 + MAX_LISTED_FILES + 1);
        assert!(detail.ends_with("• and 3 more"));
    }
}
