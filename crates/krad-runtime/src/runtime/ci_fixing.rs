use krad_chat::{new_session_id, LiveSession};
use krad_domain::review::{HeadRepository, JobId};
use krad_domain::{
    AgentPreset, EnvironmentName, GroupId, NodeEdit, NodeKind, NodePlace, Prompt, TerminalId,
    WorkerSpawn,
};
use krad_protocol::{ChatCommand, FixOutcome, Reply, ReviewAnswer};
use krad_repo::RepoError;
use std::path::{Path, PathBuf};

use super::working::agent;
use super::Runtime;
use crate::chats::{ChatBrief, ChatRole, ChatStart};
use crate::git::{Act, ActOrder, Acted, Asker, AskpassPlan};
use crate::reviews::{
    branch_owner, pick_branch, shares_repository, ChatSeat, FixOwner, FixRead, ReviewReply,
};
use crate::workers::worker_node;

#[derive(Debug)]
pub(crate) struct PendingFix {
    reply: ReviewReply,
    job: JobId,
    harness: AgentPreset,
    read: FixRead,
}

fn fix_task(branch: &str) -> String {
    format!("Fix the CI of {branch}")
}

impl Runtime {
    pub(super) fn deliver_fix(
        &mut self,
        reply: ReviewReply,
        (job, harness): (JobId, AgentPreset),
        read: Result<FixRead, String>,
    ) {
        let read = match read {
            Ok(read) => read,
            Err(message) => return self.answer_fix(reply, job, FixOutcome::Failed { message }),
        };
        if !read.head.lives_in(&read.project) {
            let fork = match read.head {
                HeadRepository::Fork(fork) => fork,
                HeadRepository::Same => None,
            };
            return self.answer_fix(reply, job, FixOutcome::InFork { fork });
        }
        let environment = reply.environment.clone();
        let fix = match self.fix_owner(&environment, &reply.root, &read.branch) {
            FixOwner::Chat(terminal) => self.send_fix(&environment, terminal, harness, &read),
            FixOwner::Group(group) => {
                self.start_fix_chat(&environment, Some(group), harness, &read)
            }
            FixOwner::Nobody => {
                let pending = PendingFix {
                    reply,
                    job,
                    harness,
                    read,
                };
                return self.open_fix_worktree(pending);
            }
        };
        self.answer_fix(reply, job, fix);
    }

    fn answer_fix(&mut self, reply: ReviewReply, job: JobId, fix: FixOutcome) {
        self.answer_review(reply, ReviewAnswer::Fixed { job, fix });
    }

    fn fix_owner(&self, environment: &EnvironmentName, root: &Path, branch: &str) -> FixOwner {
        let Some(data) = self.environments.data(environment) else {
            return FixOwner::Nobody;
        };
        let Some(repository) = self.git.at(&data.host, root) else {
            return FixOwner::Nobody;
        };
        let groups = data
            .groups
            .iter()
            .filter(|group| shares_repository(repository, &group.root))
            .map(|group| {
                let checked_out = self
                    .git
                    .at(&data.host, &group.root)
                    .and_then(|git| git.status.branch.as_deref())
                    .or_else(|| group.worktree.as_ref().map(|origin| origin.branch.as_str()));
                (group.id, checked_out)
            });
        let chats = data
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Chat)
            .map(|node| ChatSeat {
                id: node.id,
                group: node.group,
                worker: data.worker(node.id).is_some(),
            });
        branch_owner(groups, chats, branch)
    }

    fn send_fix(
        &mut self,
        environment: &EnvironmentName,
        terminal: TerminalId,
        harness: AgentPreset,
        read: &FixRead,
    ) -> FixOutcome {
        let queued = ChatCommand::Queue {
            chat: agent(environment, terminal),
            prompt: Prompt::from(read.prompt.clone()),
        };
        if self.execute_chat(queued) == Some(Reply::Accepted) {
            return FixOutcome::Sent {
                branch: read.branch.clone(),
            };
        }
        let group = self
            .environments
            .data(environment)
            .and_then(|data| data.group_of(terminal))
            .map(|group| group.id);
        self.start_fix_chat(environment, group, harness, read)
    }

    fn start_fix_chat(
        &mut self,
        environment: &EnvironmentName,
        group: Option<GroupId>,
        harness: AgentPreset,
        read: &FixRead,
    ) -> FixOutcome {
        let Some(data) = self.environments.data(environment) else {
            return FixOutcome::NoChat;
        };
        let terminal = TerminalId(data.next_id);
        let session = data.next_session_id();
        let label = self.settings.current.acp.harness_name(harness);
        let opened = NodeEdit::Open {
            node: Box::new(worker_node(terminal, &label, harness, group)),
            place: NodePlace::Worker {
                session,
                spawn: WorkerSpawn {
                    lead: None,
                    task: fix_task(&read.branch),
                },
                cloud: None,
            },
        };
        self.environments
            .edit(environment.clone(), &opened.into(), None, &mut self.outbox);
        let placed = self
            .environments
            .data(environment)
            .and_then(|data| data.session_of(terminal));
        if placed.is_none() {
            return FixOutcome::NoChat;
        }
        let start = ChatStart {
            harness,
            brief: ChatBrief {
                role: ChatRole::Agent,
                ..ChatBrief::default()
            },
            prepared: None,
            session: LiveSession::Start(new_session_id()),
            messages: Vec::new(),
            queued: vec![Prompt::from(read.prompt.clone())],
            notices: Vec::new(),
        };
        self.open_chat(&agent(environment, terminal), start);
        FixOutcome::Started {
            branch: read.branch.clone(),
        }
    }

    fn open_fix_worktree(&mut self, pending: PendingFix) {
        let environment = pending.reply.environment.clone();
        let Some(host) = self.environments.host_of(&environment) else {
            return self.answer_fix(pending.reply, pending.job, FixOutcome::NoRepository);
        };
        let Some(git) = self.git.at(&host, &pending.reply.root) else {
            return self.answer_fix(pending.reply, pending.job, FixOutcome::NoRepository);
        };
        let Some(branch) = pick_branch(&git.refs.branches, &pending.read.branch).cloned() else {
            let branch = pending.read.branch.clone();
            return self.answer_fix(pending.reply, pending.job, FixOutcome::NoBranch { branch });
        };
        let (reply, job) = (pending.reply.clone(), pending.job);
        let order = ActOrder {
            environment: environment.clone(),
            group: self.group_with_root(&environment, &pending.reply.root),
            link: self.hosts.link(&host),
            repo: git.repo_root.clone(),
            askpass: AskpassPlan::Off,
            act: Act::OpenBranchWorktree(branch),
            asker: Asker::CiFix(Box::new(pending)),
        };
        if let Reply::Refused { message, .. } = self.start_act(order) {
            self.answer_fix(reply, job, FixOutcome::Failed { message });
        }
    }

    pub(super) fn fix_worktree_made(
        &mut self,
        pending: PendingFix,
        repo: PathBuf,
        acted: Result<Acted, RepoError>,
    ) {
        let worktree = match acted {
            Ok(Acted::Opened(worktree) | Acted::Created(worktree)) => worktree,
            Ok(Acted::Done | Acted::Stale | Acted::Removed) => {
                return self.answer_fix(pending.reply, pending.job, FixOutcome::NoChat);
            }
            Err(error) => {
                let message = error.to_string();
                let fix = FixOutcome::Failed { message };
                return self.answer_fix(pending.reply, pending.job, fix);
            }
        };
        let environment = pending.reply.environment.clone();
        let group = self.open_worktree_group(&environment, repo, worktree);
        let fix = self.start_fix_chat(&environment, group, pending.harness, &pending.read);
        self.answer_fix(pending.reply, pending.job, fix);
    }
}
