use super::Edited;
use crate::agent::{AgentPreset, AgentState};
use crate::bookmark::ChatBookmarks;
use crate::checkpoint::ChatCheckpoints;
use crate::cloud::{CloudPlacement, CloudProcess};
use crate::environment::{EnvironmentData, NodeKind, TerminalNode};
use crate::group::{ExtensionTab, FileOpening, FileView, WorkerSpawn};
use crate::ids::{GroupId, ResumeId, SessionId, TerminalId};
use crate::model::ChatChoice;
use crate::text_scale::TextScale;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum NodeEdit {
    Open {
        node: Box<TerminalNode>,
        place: NodePlace,
    },
    OpenFile {
        group: Option<GroupId>,
        path: PathBuf,
        view: FileView,
        opening: FileOpening,
    },
    PinFile {
        terminal: TerminalId,
    },
    OpenExtensionTab {
        group: Option<GroupId>,
        tab: ExtensionTab,
        title: String,
    },
    Close {
        terminal: TerminalId,
    },
    Reserve {
        terminal: TerminalId,
    },
    Set {
        terminal: TerminalId,
        field: NodeField,
    },
    ForgetWorker {
        terminal: TerminalId,
    },
    ResumeWorker {
        terminal: TerminalId,
        resume: ResumeId,
    },
    ArchiveChat {
        terminal: TerminalId,
        #[cfg_attr(feature = "ts", ts(as = "crate::ts_shapes::SerdeSystemTime"))]
        at: SystemTime,
    },
    ReopenArchivedChat {
        terminal: TerminalId,
    },
    DeleteArchivedChat {
        terminal: TerminalId,
    },
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "at", rename_all = "snake_case")]
pub enum NodePlace {
    Tab {
        anchor: Option<TerminalId>,
    },
    Worker {
        session: SessionId,
        spawn: WorkerSpawn,
        cloud: Option<CloudPlacement>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "field", content = "value", rename_all = "snake_case")]
pub enum NodeField {
    State(AgentState),
    Title(String),
    Kind(NodeKind),
    Harness(Option<AgentPreset>),
    Resume(Option<ResumeId>),
    TextScale(TextScale),
    Bookmarks(ChatBookmarks),
    Choice(ChatChoice),
    Checkpoints(ChatCheckpoints),
    Cloud(Option<CloudProcess>),
}

impl NodeField {
    fn set_on(&self, node: &mut TerminalNode) {
        match self {
            Self::State(state) => node.state = *state,
            Self::Title(title) => title.clone_into(&mut node.title),
            Self::Kind(kind) => node.kind = *kind,
            Self::Harness(harness) => node.harness = *harness,
            Self::Resume(resume) => node.resume.clone_from(resume),
            Self::TextScale(scale) => node.text_scale = *scale,
            Self::Bookmarks(bookmarks) => node.bookmarks.clone_from(bookmarks),
            Self::Choice(choice) => node.choice.clone_from(choice),
            Self::Checkpoints(checkpoints) => node.checkpoints.clone_from(checkpoints),
            Self::Cloud(cloud) => node.cloud.clone_from(cloud),
        }
    }
}

impl NodeEdit {
    pub(super) const fn terminal(&self) -> Option<TerminalId> {
        match self {
            Self::Set { terminal, .. } => Some(*terminal),
            _ => None,
        }
    }

    pub(super) fn apply_to(&self, data: &mut EnvironmentData) -> Edited {
        match self {
            Self::Open { node, place } => Edited::of(open(data, node, place)),
            Self::OpenFile {
                group,
                path,
                view,
                opening,
            } => Edited::Opened(data.open_file_as(*group, path, view.clone(), *opening)),
            Self::PinFile { terminal } => {
                data.pin_file(*terminal);
                Edited::Applied
            }
            Self::OpenExtensionTab { group, tab, title } => {
                Edited::Opened(data.open_extension_tab(*group, tab.clone(), title))
            }
            Self::Close { terminal } => {
                let known = data.nodes.iter().any(|node| node.id == *terminal);
                data.retire_chat_usage(*terminal);
                data.drop_terminal(*terminal);
                Edited::of(known)
            }
            Self::Reserve { terminal } => {
                data.next_id = data.next_id.max(terminal.as_u64() + 1);
                Edited::Applied
            }
            Self::Set { terminal, field } => {
                let Some(node) = data.nodes.iter_mut().find(|node| node.id == *terminal) else {
                    return Edited::Ignored;
                };
                field.set_on(node);
                Edited::Applied
            }
            Self::ForgetWorker { terminal } => {
                data.forget_worker(*terminal);
                Edited::Applied
            }
            Self::ResumeWorker { terminal, resume } => {
                let Some(node) = data.nodes.iter_mut().find(|node| node.id == *terminal) else {
                    return Edited::Ignored;
                };
                node.resume = Some(resume.clone());
                data.resume_worker_usage(*terminal);
                Edited::Applied
            }
            Self::ArchiveChat { terminal, at } => Edited::of(data.archive_chat(*terminal, *at)),
            Self::ReopenArchivedChat { terminal } => data
                .reopen_archived_chat(*terminal)
                .map_or(Edited::Ignored, Edited::Resumed),
            Self::DeleteArchivedChat { terminal } => {
                Edited::Deleted(data.archive.delete_chat(*terminal).into_iter().collect())
            }
            Self::Unknown => Edited::Ignored,
        }
    }
}

fn open(data: &mut EnvironmentData, node: &TerminalNode, place: &NodePlace) -> bool {
    if data.nodes.iter().any(|kept| kept.id == node.id) {
        return false;
    }
    let id = node.id;
    data.next_id = data.next_id.max(id.as_u64() + 1);
    data.nodes.push(node.clone());
    match place {
        NodePlace::Tab { anchor } => data.place_terminal(id, *anchor),
        NodePlace::Worker {
            session,
            spawn,
            cloud,
        } => {
            data.open_worker_session_as(id, *session, spawn.clone());
            if let Some(opened) = data.sessions.iter_mut().find(|kept| kept.id == *session) {
                opened.cloud.clone_from(cloud);
            }
        }
    }
    true
}
