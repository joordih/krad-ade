use super::Edited;
use crate::archive::{ArchiveRetention, ArchivedSessionId};
use crate::cloud::CloudPlacement;
use crate::environment::EnvironmentData;
use crate::group::{
    DiffComment, SessionChanges, SessionLead, SessionLook, SessionPanes, SessionSpec,
};
use crate::ids::{GroupId, SessionId, TerminalId};
use crate::kraddie::KraddieId;
use crate::mosaic::MosaicState;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum SessionEdit {
    Open {
        session: SessionId,
        group: Option<GroupId>,
        name: String,
        shown: bool,
    },
    OpenForKraddie {
        session: SessionId,
        group: Option<GroupId>,
        kraddie: KraddieId,
        name: String,
    },
    Close {
        session: SessionId,
    },
    Rename {
        session: SessionId,
        name: String,
    },
    SetLook {
        session: SessionId,
        look: SessionLook,
    },
    SetCloud {
        session: SessionId,
        cloud: Option<CloudPlacement>,
    },
    Select {
        session: SessionId,
    },
    Step {
        forward: bool,
    },
    Reveal {
        terminal: TerminalId,
    },
    RememberChat {
        terminal: TerminalId,
    },
    MoveTabs {
        session: SessionId,
        tabs: MosaicState,
    },
    SetPanes {
        session: SessionId,
        panes: SessionPanes,
    },
    SetChanges {
        session: SessionId,
        changes: Option<SessionChanges>,
    },
    SetDiffComments {
        session: SessionId,
        comments: Vec<DiffComment>,
    },
    ToggleAttention {
        session: SessionId,
    },
    MakeLead {
        session: SessionId,
        lead: SessionLead,
    },
    DismissLead {
        session: SessionId,
    },
    NoteKraddieMentions {
        chat: TerminalId,
        kraddies: Vec<KraddieId>,
    },
    RenameKraddie {
        old: KraddieId,
        new: KraddieId,
    },
    Archive {
        session: SessionId,
        at: SystemTime,
    },
    Leave {
        session: SessionId,
    },
    Reopen {
        archived: ArchivedSessionId,
    },
    DeleteArchived {
        archived: ArchivedSessionId,
    },
    ForgetGroupArchive {
        group: GroupId,
    },
    ForgetArchive,
    ExpireArchive {
        retention: ArchiveRetention,
        now: SystemTime,
    },
    #[serde(other)]
    Unknown,
}

impl SessionEdit {
    pub(super) const fn session(&self) -> Option<SessionId> {
        match self {
            Self::Rename { session, .. }
            | Self::SetLook { session, .. }
            | Self::SetCloud { session, .. }
            | Self::MoveTabs { session, .. }
            | Self::SetPanes { session, .. }
            | Self::SetChanges { session, .. }
            | Self::SetDiffComments { session, .. }
            | Self::ToggleAttention { session }
            | Self::MakeLead { session, .. } => Some(*session),
            _ => None,
        }
    }

    pub(super) fn apply_to(&self, data: &mut EnvironmentData) -> Edited {
        match self {
            Self::Open {
                session,
                group,
                name,
                shown,
            } => {
                let before = data.active_session;
                let opened = data.open_session(*session, *group, spec(name));
                if !shown {
                    data.active_session = before;
                }
                Edited::of(opened)
            }
            Self::OpenForKraddie {
                session,
                group,
                kraddie,
                name,
            } => {
                Edited::of(data.open_kraddie_session(*session, *group, kraddie.clone(), spec(name)))
            }
            Self::Close { session } => Edited::of(data.close_session(*session)),
            Self::Select { session } => Edited::of(data.select_session(*session)),
            Self::Step { forward } => Edited::of(data.step_session(*forward)),
            Self::Reveal { terminal } => {
                let known = data.session_of(*terminal).is_some();
                data.reveal_terminal(*terminal);
                Edited::of(known)
            }
            Self::RememberChat { terminal } => {
                data.remember_chat(*terminal);
                Edited::Applied
            }
            Self::ToggleAttention { session } => {
                let known = data.session(*session).is_some();
                data.toggle_session_attention(*session);
                Edited::of(known)
            }
            Self::DismissLead { session } => data
                .dismiss_lead(*session)
                .map_or(Edited::Ignored, Edited::LeadDismissed),
            Self::NoteKraddieMentions { chat, kraddies } => {
                Edited::of(data.note_kraddie_mentions(*chat, kraddies))
            }
            Self::RenameKraddie { old, new } => {
                data.rename_kraddie(old, new);
                Edited::Applied
            }
            Self::Archive { .. }
            | Self::Leave { .. }
            | Self::Reopen { .. }
            | Self::DeleteArchived { .. }
            | Self::ForgetGroupArchive { .. }
            | Self::ForgetArchive
            | Self::ExpireArchive { .. } => self.apply_to_archive(data),
            Self::Unknown => Edited::Ignored,
            Self::Rename { .. }
            | Self::SetLook { .. }
            | Self::SetCloud { .. }
            | Self::MoveTabs { .. }
            | Self::SetPanes { .. }
            | Self::SetChanges { .. }
            | Self::SetDiffComments { .. }
            | Self::MakeLead { .. } => Edited::of(self.set_on_session(data)),
        }
    }

    fn apply_to_archive(&self, data: &mut EnvironmentData) -> Edited {
        match self {
            Self::Archive { session, at } => {
                Edited::of(data.archive_session(*session, *at).is_some())
            }
            Self::Leave { session } => {
                data.archive.leave_session(*session);
                Edited::Applied
            }
            Self::Reopen { archived } => data
                .reopen_archived_session(*archived)
                .map_or(Edited::Ignored, Edited::Reopened),
            Self::DeleteArchived { archived } => {
                Edited::Deleted(data.archive.delete_session(*archived))
            }
            Self::ForgetGroupArchive { group } => {
                Edited::Deleted(data.archive.forget_group(*group))
            }
            Self::ForgetArchive => Edited::Deleted(data.archive.forget_all()),
            Self::ExpireArchive { retention, now } => {
                Edited::Deleted(data.archive.expire(*retention, *now))
            }
            _ => Edited::Ignored,
        }
    }

    fn set_on_session(&self, data: &mut EnvironmentData) -> bool {
        let Some(id) = self.session() else {
            return false;
        };
        let Some(session) = data.sessions.iter_mut().find(|session| session.id == id) else {
            return false;
        };
        match self {
            Self::Rename { name, .. } => name.clone_into(&mut session.name),
            Self::SetLook { look, .. } => session.look = *look,
            Self::SetCloud { cloud, .. } => session.cloud.clone_from(cloud),
            Self::MoveTabs { tabs, .. } => session.tabs.clone_from(tabs),
            Self::SetPanes { panes, .. } => session.panes.clone_from(panes),
            Self::SetChanges { changes, .. } => session.changes.clone_from(changes),
            Self::SetDiffComments { comments, .. } => {
                session.diff_comments.clone_from(comments);
            }
            Self::MakeLead { lead, .. } => session.lead = Some(lead.clone()),
            _ => return false,
        }
        true
    }
}

fn spec(name: &str) -> SessionSpec {
    SessionSpec {
        name: name.to_owned(),
    }
}
