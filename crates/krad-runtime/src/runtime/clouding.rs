use krad_domain::{AgentPreset, CloudRunnerId, EnvironmentName, GroupId, SessionEdit, SessionId};
use krad_protocol::{
    CloudCommand, CloudDraftId, CloudNews, CloudOpened, ErrorCode, EventKind, PatchAnswer, Reply,
};
use krad_repo::{resolved_remote_url, working_patch_with_untracked, ORIGIN_REMOTE};

use super::working::agent;
use super::{refused, Runtime, UNSUPPORTED_COMMAND};
use crate::cloud::{
    carried_chats, carry_chats, cloud_chat_harness, cloud_session_name, draft_nonce, fresh_start,
    holds_cloud_work, offered_patch, prepare_workspace, reached_limit, sent_session_name,
    session_slug, upstream_branch, upstream_remote, CarriedChat, CarrySource, CloudDone,
    CloudDraft, DraftFacts, PreparedWorkspace, WorkspaceError, WorkspaceRequest,
};
use crate::jobs::JobDone;
use krad_protocol::CloudChatStart;

const NOT_STARTED: &str = "could not start";

impl From<CloudDone> for JobDone {
    fn from(done: CloudDone) -> Self {
        Self::Cloud(Box::new(done))
    }
}

impl Runtime {
    pub(super) fn execute_cloud(&mut self, command: CloudCommand) -> Reply {
        match command {
            CloudCommand::Create {
                environment,
                group,
                runner,
            } => self.create_cloud_session((environment, group, runner), None, Vec::new()),
            CloudCommand::Send {
                environment,
                session,
                runner,
            } => self.send_to_cloud(environment, session, runner),
            CloudCommand::AnswerPatch { draft, answer } => self.answer_patch(draft, answer),
            CloudCommand::Push {
                environment,
                session,
            } => self.push_cloud_branch(&environment, session),
            CloudCommand::OpenRequest {
                environment,
                session,
            } => self.open_cloud_request(&environment, session),
            CloudCommand::BringHome {
                environment,
                session,
            } => self.bring_cloud_home(&environment, session),
            CloudCommand::Unknown => return refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND),
        }
        Reply::Accepted
    }

    pub(super) fn cloud_news(&mut self, news: CloudNews) {
        self.outbox.push(EventKind::CloudNews(news));
    }

    pub(super) fn run_cloud_job(&mut self, work: impl FnOnce() -> CloudDone + Send + 'static) {
        if let Err(error) = self.jobs.run(move || work().into()) {
            tracing::warn!(%error, "a cloud session job could not start");
            let error = format!("{NOT_STARTED}: {error}");
            self.cloud_news(CloudNews::ResultFailed { error });
        }
    }

    fn running_cloud_sessions(&self) -> usize {
        self.environments
            .iter()
            .flat_map(|data| {
                data.sessions
                    .iter()
                    .filter(|session| holds_cloud_work(session, &data.nodes))
            })
            .count()
    }

    fn send_to_cloud(
        &mut self,
        environment: EnvironmentName,
        session: SessionId,
        runner: CloudRunnerId,
    ) {
        let Some(data) = self.environments.data(&environment) else {
            return self.cloud_news(CloudNews::NoSession);
        };
        let Some(found) = data.session(session) else {
            return self.cloud_news(CloudNews::NoSession);
        };
        if found.cloud.is_some() {
            return self.cloud_news(CloudNews::AlreadyCloud);
        }
        let Some(group) = found.group else {
            return self.cloud_news(CloudNews::NoGroup);
        };
        let name = sent_session_name(&found.name);
        let chats = &self.chats;
        let messages = |terminal| chats.messages(&agent(&environment, terminal));
        let source = CarrySource {
            data,
            link: self.hosts.link(&data.host),
            acp: &self.settings.current.acp,
            messages: &messages,
        };
        let carried = carried_chats(&source, session);
        tracing::info!(%runner, chats = carried.len(), "sending a session to the cloud");
        self.create_cloud_session((environment, group, runner), Some(name), carried);
    }

    #[tracing::instrument(skip_all, fields(%runner))]
    fn create_cloud_session(
        &mut self,
        (environment, group, runner): (EnvironmentName, GroupId, CloudRunnerId),
        name: Option<String>,
        carried: Vec<CarriedChat>,
    ) {
        let max = self.settings.current.cloud.max_sessions;
        if reached_limit(self.running_cloud_sessions(), max) {
            return self.cloud_news(CloudNews::AtLimit);
        }
        let Some(data) = self.environments.data(&environment) else {
            return;
        };
        let Some(found) = data.group(group) else {
            return;
        };
        let upstream = self
            .git
            .at(&data.host, &found.root)
            .and_then(|git| git.status.upstream.clone());
        let link = self.hosts.link(&data.host);
        let remote = upstream_remote(upstream.as_deref())
            .unwrap_or(ORIGIN_REMOTE)
            .to_owned();
        let root = found.root.clone();
        let facts = Box::new(DraftFacts {
            environment,
            group,
            runner,
            group_name: found.name.clone(),
            name,
            upstream,
            carried,
        });
        self.run_cloud_job(move || {
            let home = std::env::home_dir();
            let url = resolved_remote_url(&link, &root, &remote, home.as_deref());
            let diff = working_patch_with_untracked(&link, &root).unwrap_or_else(|error| {
                tracing::debug!(%error, "no uncommitted changes to offer");
                String::new()
            });
            CloudDone::RemoteRead { facts, url, diff }
        });
    }

    pub(super) fn cloud_done(&mut self, done: CloudDone) {
        match done {
            CloudDone::RemoteRead { facts, url, diff } => self.remote_read(*facts, url, diff),
            CloudDone::Prepared { draft, prepared } => self.cloud_prepared(*draft, prepared),
            results => self.cloud_result(results),
        }
    }

    fn remote_read(
        &mut self,
        facts: DraftFacts,
        url: Result<Option<String>, krad_repo::RepoError>,
        diff: String,
    ) {
        let remote_url = match url {
            Ok(Some(url)) => url,
            Ok(None) => return self.cloud_news(CloudNews::NoRemote),
            Err(error) => {
                let error = error.to_string();
                return self.cloud_news(CloudNews::RemoteUnread { error });
            }
        };
        let ordinal = self
            .environments
            .data(&facts.environment)
            .map_or(0, |data| {
                data.sessions_of(Some(facts.group))
                    .filter(|session| session.cloud.is_some())
                    .count()
            })
            + 1;
        let draft = CloudDraft {
            name: facts
                .name
                .unwrap_or_else(|| cloud_session_name(&facts.group_name, ordinal)),
            request: WorkspaceRequest {
                runner: facts.runner,
                remote_url,
                upstream_branch: upstream_branch(facts.upstream.as_deref()).map(str::to_owned),
                session_slug: session_slug(&facts.group_name, &draft_nonce()),
                patch: None,
            },
            environment: facts.environment,
            group: facts.group,
            carried: facts.carried,
        };
        match offered_patch(diff) {
            Some(patch) => {
                let draft = self.cloud.ask(draft, patch);
                self.outbox.push(EventKind::CloudPatchOffered { draft });
            }
            None => self.queue_cloud_session(draft),
        }
    }

    fn answer_patch(&mut self, draft: CloudDraftId, answer: PatchAnswer) {
        let Some((mut draft, patch)) = self.cloud.answered(draft) else {
            return;
        };
        match answer {
            PatchAnswer::Cancel => return,
            PatchAnswer::Bring => draft.request.patch = Some(patch),
            PatchAnswer::Leave => {}
        }
        self.queue_cloud_session(draft);
    }

    fn queue_cloud_session(&mut self, draft: CloudDraft) {
        let runner = draft.request.runner;
        self.cloud_news(CloudNews::Preparing { runner });
        self.cloud.wait(draft);
        if self.runners.directory.get(runner).is_some() {
            self.start_waiting_cloud_sessions(runner);
        } else {
            self.ensure_cloud_runner(runner);
        }
    }

    fn ensure_cloud_runner(&mut self, runner: CloudRunnerId) {
        if let Reply::Refused { message, .. } = self.ensure_runner(runner) {
            self.fail_waiting_cloud_sessions(runner, &message);
        }
    }

    pub(super) fn start_waiting_cloud_sessions(&mut self, runner: CloudRunnerId) {
        let Some(link) = self.runners.directory.get(runner).map(|ready| ready.link) else {
            return;
        };
        for mut draft in self.cloud.take_waiting(runner) {
            let link = link.clone();
            let request = draft.request.clone();
            let carried = std::mem::take(&mut draft.carried);
            let draft = Box::new(draft);
            self.run_cloud_job(move || {
                let prepared = prepare_workspace(&link, &request).map(|prepared| {
                    let starts = carry_chats(&link, &prepared.placement.worktree, carried);
                    (prepared, starts)
                });
                CloudDone::Prepared { draft, prepared }
            });
        }
    }

    pub(super) fn fail_waiting_cloud_sessions(&mut self, runner: CloudRunnerId, reason: &str) {
        if self.cloud.take_waiting(runner).is_empty() {
            return;
        }
        let reason = reason.to_owned();
        self.cloud_news(CloudNews::RunnerFailed { runner, reason });
    }

    fn cloud_prepared(
        &mut self,
        draft: CloudDraft,
        prepared: Result<(PreparedWorkspace, Vec<CloudChatStart>), WorkspaceError>,
    ) {
        let (prepared, starts) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                tracing::warn!(%error, "the cloud session's workspace could not be prepared");
                let error = error.to_string();
                return self.cloud_news(CloudNews::NotPrepared { error });
            }
        };
        if let Some(problem) = prepared.patch_problem {
            self.cloud_news(CloudNews::PatchProblem { problem });
        }
        let CloudDraft {
            environment,
            group,
            name,
            ..
        } = draft;
        let Some(session) = self
            .environments
            .data(&environment)
            .filter(|data| data.group(group).is_some())
            .map(krad_domain::EnvironmentData::next_session_id)
        else {
            return;
        };
        let runner = prepared.placement.runner;
        let cloud = Some(prepared.placement);
        for edit in [
            SessionEdit::Open {
                session,
                group: Some(group),
                name,
                shown: true,
            },
            SessionEdit::SetCloud { session, cloud },
        ] {
            self.environments
                .edit(environment.clone(), &edit.into(), None, &mut self.outbox);
        }
        tracing::info!(%runner, session = session.as_u64(), "cloud session opened");
        let starts = if starts.is_empty() {
            vec![fresh_start(self.default_cloud_harness(), None)]
        } else {
            starts
        };
        self.outbox.push(EventKind::CloudSessionOpened(CloudOpened {
            environment,
            session,
            group,
            starts,
        }));
    }

    fn default_cloud_harness(&self) -> AgentPreset {
        let settings = &self.settings.current;
        let candidates = AgentPreset::chat_harnesses(&settings.acp.all_agents());
        cloud_chat_harness(settings.default_harness, &candidates)
    }
}
