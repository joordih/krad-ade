use crate::archive::{ArchivedChat, Reopened, ResumedChat};
use crate::environment::{EnvironmentChange, EnvironmentData};
use crate::group::SessionLead;
use crate::ids::{GroupId, SessionId, TerminalId};
use std::collections::HashMap;

mod group;
mod node;
mod session;

pub use group::GroupEdit;
pub use node::{NodeEdit, NodeField, NodePlace};
pub use session::SessionEdit;

#[derive(Clone, Debug, PartialEq)]
pub enum EnvironmentEdit {
    Session(SessionEdit),
    Group(GroupEdit),
    Node(NodeEdit),
}

impl From<SessionEdit> for EnvironmentEdit {
    fn from(edit: SessionEdit) -> Self {
        Self::Session(edit)
    }
}

impl From<GroupEdit> for EnvironmentEdit {
    fn from(edit: GroupEdit) -> Self {
        Self::Group(edit)
    }
}

impl From<NodeEdit> for EnvironmentEdit {
    fn from(edit: NodeEdit) -> Self {
        Self::Node(edit)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Edited {
    Applied,
    Ignored,
    Opened(TerminalId),
    Reopened(Reopened),
    Resumed(ResumedChat),
    Deleted(Vec<ArchivedChat>),
    LeadDismissed(SessionLead),
}

impl Edited {
    #[must_use]
    pub const fn applied(&self) -> bool {
        !matches!(self, Self::Ignored)
    }

    #[must_use]
    pub fn opened(&self) -> Option<TerminalId> {
        match self {
            Self::Opened(terminal) => Some(*terminal),
            _ => None,
        }
    }

    #[must_use]
    pub fn deleted(self) -> Vec<ArchivedChat> {
        match self {
            Self::Deleted(chats) => chats,
            _ => Vec::new(),
        }
    }

    fn of(applied: bool) -> Self {
        if applied {
            Self::Applied
        } else {
            Self::Ignored
        }
    }
}

enum Reach {
    Node(TerminalId),
    Session(SessionId),
    Group(GroupId),
    Whole,
}

impl EnvironmentEdit {
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(
            self,
            Self::Session(SessionEdit::Unknown)
                | Self::Group(GroupEdit::Unknown)
                | Self::Node(NodeEdit::Unknown)
        )
    }

    fn reach(&self) -> Reach {
        match self {
            Self::Session(edit) => edit.session().map_or(Reach::Whole, Reach::Session),
            Self::Group(edit) => edit.group().map_or(Reach::Whole, Reach::Group),
            Self::Node(edit) => edit.terminal().map_or(Reach::Whole, Reach::Node),
        }
    }
}

impl EnvironmentData {
    pub fn edit(&mut self, edit: &EnvironmentEdit) -> Edited {
        match edit {
            EnvironmentEdit::Session(edit) => edit.apply_to(self),
            EnvironmentEdit::Group(edit) => edit.apply_to(self),
            EnvironmentEdit::Node(edit) => edit.apply_to(self),
        }
    }

    pub fn edit_with_changes(
        &mut self,
        edit: &EnvironmentEdit,
    ) -> (Edited, Vec<EnvironmentChange>) {
        match edit.reach() {
            Reach::Node(terminal) => {
                let edited = self.edit(edit);
                let changes = self
                    .nodes
                    .iter()
                    .position(|node| node.id == terminal)
                    .map(|index| EnvironmentChange::Node {
                        index,
                        node: self.nodes[index].clone(),
                    })
                    .into_iter()
                    .collect();
                (edited, changes)
            }
            Reach::Session(session) => {
                let edited = self.edit(edit);
                let changes = self
                    .sessions
                    .iter()
                    .position(|kept| kept.id == session)
                    .map(|index| EnvironmentChange::Session {
                        index,
                        session: self.sessions[index].clone(),
                    })
                    .into_iter()
                    .collect();
                (edited, changes)
            }
            Reach::Group(group) => {
                let edited = self.edit(edit);
                let changes = self
                    .groups
                    .iter()
                    .position(|kept| kept.id == group)
                    .map(|index| EnvironmentChange::Group {
                        index,
                        group: self.groups[index].clone(),
                    })
                    .into_iter()
                    .collect();
                (edited, changes)
            }
            Reach::Whole => self.edit_whole(edit),
        }
    }

    fn edit_whole(&mut self, edit: &EnvironmentEdit) -> (Edited, Vec<EnvironmentChange>) {
        let before = self.without_meta();
        let kept_meta: Vec<TerminalId> = self.meta.keys().copied().collect();
        let edited = self.edit(edit);
        let mut changes = EnvironmentChange::between_items(&before, self);
        changes.extend(
            kept_meta
                .into_iter()
                .filter(|terminal| !self.meta.contains_key(terminal))
                .map(|terminal| EnvironmentChange::MetaRemoved { terminal }),
        );
        (edited, changes)
    }

    fn without_meta(&self) -> Self {
        Self {
            name: self.name.clone(),
            host: self.host.clone(),
            nodes: self.nodes.clone(),
            meta: HashMap::new(),
            next_id: self.next_id,
            view: self.view,
            focus: self.focus,
            title: self.title.clone(),
            groups: self.groups.clone(),
            sessions: self.sessions.clone(),
            active_session: self.active_session,
            side_panel: crate::side_panel::SidePanelLayout::default(),
            note_tabs: Vec::new(),
            extension_views: crate::group::ShownExtensionViews::default(),
            retired_spend: self.retired_spend.clone(),
            variant_runs: self.variant_runs.clone(),
            archive: self.archive.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
