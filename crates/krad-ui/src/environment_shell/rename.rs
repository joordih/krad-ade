use gpui_kit::Context;
use krad_domain::{EnvironmentName, GroupId, TerminalId};
use krad_repo::EntryKind;
use std::path::{Path, PathBuf};

use super::EnvironmentShell;

pub(super) const KEY_ESCAPE: &str = "escape";
pub(super) const KEY_ENTER: &str = "enter";
const KEY_BACKSPACE: &str = "backspace";
const RENAME_CARET: &str = "▏";

#[derive(Clone, PartialEq, Eq)]
enum RenameTarget {
    Terminal(TerminalId),
    Environment(EnvironmentName),
    Worktree(GroupId),
    File(PathBuf),
    NewEntry(PathBuf, EntryKind),
}

pub(super) struct RenameDraft {
    target: RenameTarget,
    text: String,
    seeded: bool,
}

impl RenameDraft {
    fn label(&self) -> String {
        format!("{}{RENAME_CARET}", self.text)
    }
}

impl EnvironmentShell {
    pub(super) fn start_rename(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        let current = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
            .map(|node| node.title.clone());
        if let Some(text) = current {
            self.rename = Some(RenameDraft {
                target: RenameTarget::Terminal(id),
                text,
                seeded: true,
            });
            cx.notify();
        }
    }

    pub(super) fn start_rename_env(&mut self, name: &EnvironmentName, cx: &mut Context<Self>) {
        let Some(live) = self.envs.get(name) else {
            return;
        };
        self.rename = Some(RenameDraft {
            target: RenameTarget::Environment(name.clone()),
            text: live.data.display_name().to_owned(),
            seeded: true,
        });
        cx.notify();
    }

    pub(super) fn start_worktree_draft(&mut self, group: GroupId, cx: &mut Context<Self>) {
        self.rename = Some(RenameDraft {
            target: RenameTarget::Worktree(group),
            text: String::new(),
            seeded: false,
        });
        cx.notify();
    }

    pub(super) fn worktree_draft_label(&self, group: GroupId) -> Option<String> {
        self.draft_label(&RenameTarget::Worktree(group))
    }

    pub(super) fn start_file_rename(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(name) = path.file_name() else {
            return;
        };
        self.rename = Some(RenameDraft {
            text: name.to_string_lossy().into_owned(),
            target: RenameTarget::File(path),
            seeded: true,
        });
        cx.notify();
    }

    pub(super) fn start_new_entry(
        &mut self,
        dir: PathBuf,
        kind: EntryKind,
        cx: &mut Context<Self>,
    ) {
        self.rename = Some(RenameDraft {
            target: RenameTarget::NewEntry(dir, kind),
            text: String::new(),
            seeded: false,
        });
        cx.notify();
    }

    pub(super) fn file_rename_label(&self, path: &Path) -> Option<String> {
        self.rename
            .as_ref()
            .filter(|draft| matches!(&draft.target, RenameTarget::File(target) if target == path))
            .map(RenameDraft::label)
    }

    pub(super) fn cancel_new_entry(&mut self) {
        if self
            .rename
            .as_ref()
            .is_some_and(|draft| matches!(draft.target, RenameTarget::NewEntry(..)))
        {
            self.rename = None;
        }
    }

    pub(super) fn new_entry_draft(&self) -> Option<(&Path, EntryKind, String)> {
        let draft = self.rename.as_ref()?;
        match &draft.target {
            RenameTarget::NewEntry(dir, kind) => Some((dir.as_path(), *kind, draft.label())),
            _ => None,
        }
    }

    pub(super) fn start_rename_focused(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.envs.get(&self.active).and_then(|live| live.focused) {
            self.start_rename(id, cx);
        }
    }

    pub(super) fn title_label(&self, id: TerminalId, title: &str) -> String {
        self.draft_label(&RenameTarget::Terminal(id))
            .unwrap_or_else(|| title.to_owned())
    }

    pub(super) fn env_label(&self, name: &EnvironmentName) -> String {
        self.draft_label(&RenameTarget::Environment(name.clone()))
            .or_else(|| {
                self.envs
                    .get(name)
                    .map(|live| live.data.display_name().to_owned())
            })
            .unwrap_or_else(|| name.as_str().to_owned())
    }

    fn draft_label(&self, target: &RenameTarget) -> Option<String> {
        self.rename
            .as_ref()
            .filter(|draft| &draft.target == target)
            .map(RenameDraft::label)
    }

    pub(super) fn ingest_rename_key(
        &mut self,
        key: &str,
        key_char: Option<&str>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(draft) = &mut self.rename else {
            return false;
        };
        match key {
            KEY_ESCAPE => self.rename = None,
            KEY_ENTER => self.commit_rename(cx),
            KEY_BACKSPACE => {
                if draft.seeded {
                    draft.text.clear();
                } else {
                    draft.text.pop();
                }
                draft.seeded = false;
            }
            _ => {
                if let Some(text) = key_char {
                    if draft.seeded {
                        draft.text.clear();
                    }
                    draft.text.push_str(text);
                    draft.seeded = false;
                }
            }
        }
        cx.notify();
        true
    }

    fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.rename.take() else {
            return;
        };
        let title = draft.text.trim();
        if title.is_empty() {
            return;
        }
        match draft.target {
            RenameTarget::Terminal(id) => {
                let env = self.active.clone();
                if let Some(node) = self
                    .envs
                    .get_mut(&env)
                    .and_then(|live| live.data.nodes.iter_mut().find(|node| node.id == id))
                {
                    title.clone_into(&mut node.title);
                }
            }
            RenameTarget::Environment(name) => {
                if let Some(live) = self.envs.get_mut(&name) {
                    live.data.title = Some(title.to_owned());
                }
            }
            RenameTarget::Worktree(group) => {
                self.create_worktree(group, title, cx);
                return;
            }
            RenameTarget::File(path) => {
                Self::rename_file(path, title, cx);
                return;
            }
            RenameTarget::NewEntry(dir, kind) => {
                Self::create_file_entry(dir, kind, title, cx);
                return;
            }
        }
        self.store();
    }
}
