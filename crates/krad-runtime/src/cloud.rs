use krad_domain::review::MergeRequest;
use krad_domain::{CloudRunnerId, EnvironmentName, GroupId, HostedOrigin, SessionId};
use krad_protocol::{CloudChatStart, CloudDraftId, CloudReach};
use krad_repo::{RepoError, Worktree};
use std::collections::HashMap;
use std::path::PathBuf;

mod carry;
mod hand_over;
mod naming;
mod results;
mod workspace;

pub(crate) use carry::{
    carried_chats, carry_chats, first_start, fresh_start, with_sent_prompt, CarriedChat,
    CarrySource, MovedChat,
};
pub(crate) use hand_over::{hand_over, HandOver, HandOverError};
pub(crate) use naming::{
    cloud_chat_harness, cloud_session_name, holds_cloud_work, new_nonce as draft_nonce,
    offered_patch, reached_limit, session_slug, upstream_branch, upstream_remote,
};
pub use naming::{new_nonce, node_process_id};
pub(crate) use results::{
    bring_home, open_request, push_branch, CloudWork, LocalRepo, ResultError,
};
pub(crate) use workspace::{
    placement_on, prepare_workspace, PreparedWorkspace, WorkspaceError, WorkspaceRequest,
};

#[derive(Clone, Debug)]
pub(crate) struct CloudDraft {
    pub(crate) environment: EnvironmentName,
    pub(crate) group: GroupId,
    pub(crate) name: String,
    pub(crate) request: WorkspaceRequest,
    pub(crate) carried: Vec<CarriedChat>,
    pub(crate) starts: Vec<CloudChatStart>,
    pub(crate) reach: CloudReach,
    pub(crate) origin: Option<HostedOrigin>,
    pub(crate) moving: Option<SessionId>,
}

#[derive(Debug)]
pub(crate) struct DraftFacts {
    pub(crate) environment: EnvironmentName,
    pub(crate) group: GroupId,
    pub(crate) runner: CloudRunnerId,
    pub(crate) group_name: String,
    pub(crate) name: Option<String>,
    pub(crate) upstream: Option<String>,
    pub(crate) carried: Vec<CarriedChat>,
    pub(crate) starts: Vec<CloudChatStart>,
    pub(crate) reach: CloudReach,
    pub(crate) moving: Option<SessionId>,
}

#[derive(Debug)]
pub(crate) enum CloudDone {
    RemoteRead {
        facts: Box<DraftFacts>,
        url: Result<Option<String>, RepoError>,
        diff: String,
    },
    Prepared {
        draft: Box<CloudDraft>,
        prepared: Result<(PreparedWorkspace, Vec<MovedChat>), WorkspaceError>,
    },
    Pushed {
        branch: String,
        pushed: Result<(), ResultError>,
    },
    RequestOpened {
        environment: EnvironmentName,
        group: Option<GroupId>,
        opened: Result<Box<MergeRequest>, ResultError>,
    },
    BroughtHome {
        environment: EnvironmentName,
        repo_root: PathBuf,
        branch: String,
        brought: Result<Worktree, ResultError>,
    },
    HandedOver {
        runner: CloudRunnerId,
        slug: String,
        handed: Result<(), HandOverError>,
    },
}

#[derive(Debug, Default)]
pub(crate) struct CloudSessions {
    next_draft: u64,
    pending: Vec<CloudDraft>,
    asking: HashMap<CloudDraftId, (CloudDraft, String)>,
}

impl CloudSessions {
    pub(crate) fn ask(&mut self, draft: CloudDraft, patch: String) -> CloudDraftId {
        self.next_draft += 1;
        let id = CloudDraftId(self.next_draft);
        self.asking.insert(id, (draft, patch));
        id
    }

    pub(crate) fn answered(&mut self, id: CloudDraftId) -> Option<(CloudDraft, String)> {
        self.asking.remove(&id)
    }

    pub(crate) fn wait(&mut self, draft: CloudDraft) {
        self.pending.push(draft);
    }

    pub(crate) fn take_waiting(&mut self, runner: CloudRunnerId) -> Vec<CloudDraft> {
        let (taken, waiting) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|draft| draft.request.runner == runner);
        self.pending = waiting;
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(runner: u64) -> CloudDraft {
        CloudDraft {
            environment: EnvironmentName::new("api".to_owned()),
            group: GroupId(1),
            name: "Cloud · api".to_owned(),
            request: WorkspaceRequest {
                runner: CloudRunnerId(runner),
                remote_url: "git@example.com:api.git".to_owned(),
                upstream_branch: None,
                session_slug: "api-1".to_owned(),
                patch: None,
            },
            carried: Vec::new(),
            starts: Vec::new(),
            reach: CloudReach::ProcessWire,
            origin: None,
            moving: None,
        }
    }

    #[test]
    fn drafts_wait_for_their_own_runner_and_a_patch_question_is_answered_once() {
        let mut cloud = CloudSessions::default();
        cloud.wait(draft(1));
        cloud.wait(draft(2));
        assert_eq!(cloud.take_waiting(CloudRunnerId(1)).len(), 1);
        assert!(cloud.take_waiting(CloudRunnerId(1)).is_empty());
        assert_eq!(cloud.take_waiting(CloudRunnerId(2)).len(), 1);
        let asked = cloud.ask(draft(1), "diff".to_owned());
        assert!(cloud.answered(asked).is_some());
        assert!(cloud.answered(asked).is_none());
    }
}
