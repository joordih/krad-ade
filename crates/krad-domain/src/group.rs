mod closing;
mod diff_comment;
mod extension_tab;
mod file_tab;
mod hunk_fingerprint;
mod lead;
mod new_session;
mod panes;
mod session;
mod session_attention;
mod session_changes;
mod session_chat;
mod session_look;
mod session_memory;
mod tab_closing;
mod worker;
pub use closing::*;
pub use diff_comment::*;
pub use extension_tab::*;
pub use file_tab::*;
pub use hunk_fingerprint::*;
pub use lead::*;
pub use new_session::*;
pub use panes::*;
pub use session::*;
pub use session_changes::*;
pub use session_look::*;
pub use session_memory::*;
pub use tab_closing::*;
pub use worker::*;

use crate::environment::EnvironmentData;
use crate::ids::{GroupId, TerminalId};
use crate::toolchain::GroupActivation;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalGroup {
    pub id: GroupId,
    pub name: String,
    pub root: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorktreeOrigin>,
    #[serde(default, skip_serializing_if = "GroupActivation::is_inherit")]
    pub activation: GroupActivation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeOrigin {
    pub repo_root: PathBuf,
    pub branch: String,
}

#[must_use]
pub fn group_name_for(root: &Path) -> String {
    root.file_name().map_or_else(
        || root.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

impl EnvironmentData {
    pub fn add_group(&mut self, root: PathBuf, worktree: Option<WorktreeOrigin>) -> GroupId {
        let id = GroupId(
            self.groups
                .iter()
                .map(|group| group.id.as_u64())
                .max()
                .map_or(1, |last| last + 1),
        );
        self.groups.push(TerminalGroup {
            id,
            name: group_name_for(&root),
            root,
            worktree,
            activation: GroupActivation::Inherit,
        });
        id
    }

    pub fn remove_group(&mut self, id: GroupId) -> Option<TerminalGroup> {
        let index = self.groups.iter().position(|group| group.id == id)?;
        let held: Vec<TerminalId> = self
            .nodes
            .iter()
            .filter(|node| node.group == Some(id))
            .map(|node| node.id)
            .collect();
        for terminal in held {
            self.drop_terminal(terminal);
        }
        let removed = self.groups.remove(index);
        self.sessions.retain(|session| session.group != Some(id));
        self.retired_spend.forget(id);
        self.variant_runs.forget_group(id);
        if self
            .active_session
            .is_some_and(|active| self.session(active).is_none())
        {
            self.active_session = self.heir_session(None);
        }
        self.enter_sessions();
        Some(removed)
    }

    #[must_use]
    pub fn group(&self, id: GroupId) -> Option<&TerminalGroup> {
        self.groups.iter().find(|group| group.id == id)
    }

    #[must_use]
    pub fn group_of(&self, terminal: TerminalId) -> Option<&TerminalGroup> {
        let id = self.nodes.iter().find(|node| node.id == terminal)?.group?;
        self.group(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::seed_environment;

    #[test]
    fn removing_a_group_closes_its_terminals() {
        let mut environment = seed_environment();
        let first = environment.add_group(PathBuf::from("/work/api"), None);
        let second = environment.add_group(PathBuf::from("/work/web"), None);
        assert_ne!(first, second);
        let terminal = environment.nodes[0].id;
        environment.nodes[0].group = Some(first);
        environment.enter_sessions();
        assert_eq!(
            environment
                .group_of(terminal)
                .map(|group| group.name.as_str()),
            Some("api")
        );
        assert!(environment.remove_group(first).is_some());
        assert_eq!(environment.nodes.len(), 2);
        assert!(environment.nodes.iter().all(|node| node.id != terminal));
        assert!(environment.session_of(terminal).is_none());
        assert!(!environment.meta.contains_key(&terminal));
    }
}
