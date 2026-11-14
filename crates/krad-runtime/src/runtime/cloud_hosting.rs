use krad_chat::{new_session_id, LiveSession};
use krad_domain::{CloudRunnerId, EnvironmentName, GroupEdit, GroupId, TerminalNode};
use krad_host::HostLink;
use krad_protocol::{
    CloudNews, CloudReach, ErrorCode, Hello, HostedSession, HostedWorkspace, Reply,
};
use std::path::Path;

use super::working::agent;
use super::{refused, Runtime};
use crate::chats::{ChatBrief, ChatRole, ChatStart};
use crate::cloud::{
    fresh_start, hand_over, placement_on, CloudDone, CloudDraft, HandOver, HandOverError,
    WorkspaceRequest,
};
use crate::runners::{OwnRunner, ReadyRunner, OWN_RUNNER};

const BUILD: &str = env!("CARGO_PKG_VERSION");
const NO_OWN_RUNNER: &str = "this runtime runs no cloud sessions of its own";
const NO_FOLDER: &str = "the runner's folder is not known";
const NO_ENVIRONMENT: &str = "the runner's environment is missing";

fn reopened(node: &TerminalNode) -> Option<ChatStart> {
    let harness = node.harness?;
    node.cloud.as_ref()?;
    let session = node
        .resume
        .clone()
        .map_or_else(|| LiveSession::Start(new_session_id()), LiveSession::Resume);
    Some(ChatStart {
        harness,
        brief: ChatBrief {
            role: ChatRole::Agent,
            ..ChatBrief::default()
        },
        prepared: None,
        session,
        messages: Vec::new(),
        queued: Vec::new(),
        notices: Vec::new(),
    })
}

impl Runtime {
    pub(super) fn adopt_own_runner(&mut self, own: Option<OwnRunner>) {
        let Some(own) = own else {
            return;
        };
        self.runners.adopt_own(own);
        self.reopen_own_cloud_chats();
    }

    fn reopen_own_cloud_chats(&mut self) {
        let Some(environment) = self.runners.own.clone() else {
            return;
        };
        let Some(data) = self.environments.data(&environment) else {
            return;
        };
        let chats: Vec<_> = data
            .nodes
            .iter()
            .filter_map(|node| Some((node.id, reopened(node)?)))
            .collect();
        tracing::info!(
            chats = chats.len(),
            "reopening the runner's own cloud chats"
        );
        for (terminal, start) in chats {
            self.open_chat(&agent(&environment, terminal), start);
        }
    }

    fn repo_group_on_runner(
        &mut self,
        environment: &EnvironmentName,
        repo: &Path,
    ) -> Option<GroupId> {
        if let Some(group) = self.group_with_root(environment, repo) {
            return Some(group);
        }
        let group = self.environments.data(environment)?.next_group_id();
        let added = GroupEdit::Add {
            group,
            root: repo.to_path_buf(),
            worktree: None,
        };
        self.environments
            .edit(environment.clone(), &added.into(), None, &mut self.outbox);
        self.group_with_root(environment, repo)
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn host_cloud_session(&mut self, hosted: HostedSession) -> Reply {
        let Some(environment) = self.runners.own.clone() else {
            return refused(ErrorCode::Unsupported, NO_OWN_RUNNER);
        };
        let HostedWorkspace {
            remote_url,
            upstream_branch,
            session_slug,
            patch,
        } = hosted.workspace;
        let request = WorkspaceRequest {
            runner: OWN_RUNNER,
            remote_url,
            upstream_branch,
            session_slug,
            patch,
        };
        let Ok(placement) = placement_on(&HostLink::Local, &request) else {
            return refused(ErrorCode::Unavailable, NO_FOLDER);
        };
        let Some(group) = self.repo_group_on_runner(&environment, &placement.repo_dir) else {
            return refused(ErrorCode::NotFound, NO_ENVIRONMENT);
        };
        tracing::info!(chats = hosted.starts.len(), "hosting a cloud session");
        self.queue_cloud_session(CloudDraft {
            environment,
            group,
            name: hosted.name,
            request,
            carried: Vec::new(),
            starts: hosted.starts,
            reach: CloudReach::ProcessWire,
        });
        Reply::Accepted
    }

    pub(super) fn hand_over_cloud_session(&mut self, ready: &ReadyRunner, mut draft: CloudDraft) {
        let runner = draft.request.runner;
        if draft.carried.is_empty() && draft.starts.is_empty() {
            draft.starts = vec![fresh_start(self.default_cloud_harness(), None)];
        }
        let over = HandOver {
            link: ready.link.clone(),
            cli: ready.cli.clone(),
            hello: Hello::paired(BUILD, ready.pairing.clone()),
            name: draft.name,
            request: draft.request,
            carried: draft.carried,
            starts: draft.starts,
        };
        self.run_cloud_job(move || CloudDone::HandedOver {
            runner,
            handed: hand_over(over),
        });
    }

    pub(super) fn cloud_handed_over(
        &mut self,
        runner: CloudRunnerId,
        handed: Result<(), HandOverError>,
    ) {
        match handed {
            Ok(()) => self.cloud_news(CloudNews::Hosted { runner }),
            Err(error) => {
                tracing::warn!(%runner, %error, "the cloud session was not handed to the runner");
                let error = error.to_string();
                self.cloud_news(CloudNews::NotPrepared { error });
            }
        }
    }
}
