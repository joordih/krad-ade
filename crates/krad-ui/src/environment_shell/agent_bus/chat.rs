use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::Context;
use krad_agents::{Request, Response};
use krad_domain::{
    chat_worker_brief, AgentPreset, EnvironmentName, GroupId, SessionId, TerminalGroup, TerminalId,
};

use super::super::agent_hooks::{WorkerOf, WorkerOrder};
use super::super::EnvironmentShell;
use super::requests::CLOSED_TERMINAL;

const NO_CHAT_INBOX: &str = "The chat has no inbox: follow agents with wait and read";
const TERMINALS_ONLY: &str = "Only agents in terminals can do that";
const NO_CHAT_GROUP: &str = "The chat's group no longer exists";

pub(super) struct ChatSpawn {
    pub(super) agent: AgentPreset,
    pub(super) worktree: Option<String>,
    pub(super) task: String,
    pub(super) lead: Option<SessionId>,
}

impl EnvironmentShell {
    pub(super) fn answer_chat(
        &mut self,
        environment: &str,
        group: GroupId,
        request: Request,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let response = match request {
            Request::List => Response::Agents(self.agent_infos(environment, None)),
            Request::Send { to, body } => self.submit_from_chat(environment, to, &body, cx),
            Request::Wait { target, .. } => {
                self.wait_for(environment, target, reply);
                return;
            }
            Request::Read { target, lines } => self.screen_tail(environment, target, lines),
            Request::Spawn {
                agent,
                worktree,
                task,
            } => {
                let spawn = ChatSpawn {
                    agent,
                    worktree,
                    task,
                    lead: None,
                };
                self.start_chat_worker(environment, group, spawn, reply, cx);
                return;
            }
            Request::Check { .. } => Response::Error(NO_CHAT_INBOX.to_owned()),
            Request::Hook { .. } | Request::Status(_) | Request::Ask { .. } => {
                Response::Error(TERMINALS_ONLY.to_owned())
            }
        };
        reply.send(response).unwrap_or_default();
    }

    fn submit_from_chat(
        &mut self,
        environment: &str,
        to: u64,
        body: &str,
        cx: &mut Context<Self>,
    ) -> Response {
        let env = EnvironmentName::new(environment.to_owned());
        let id = TerminalId(to);
        if self.queue_chat_prompt(&env, id, body, cx) {
            return Response::Submitted { terminal: to };
        }
        let running = self
            .envs
            .get(&env)
            .and_then(|live| live.sessions.get(&id))
            .is_some_and(|session| session.state.is_running());
        if !running {
            return Response::Error(CLOSED_TERMINAL.to_owned());
        }
        self.submit_text_into(&env, id, body);
        Response::Submitted { terminal: to }
    }

    pub(super) fn start_chat_worker(
        &mut self,
        environment: &str,
        group: GroupId,
        spawn: ChatSpawn,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let env = EnvironmentName::new(environment.to_owned());
        match self.chat_worker_order(&env, group, spawn) {
            Ok((group, order)) => self.start_worker(&env, &group, order, reply, cx),
            Err(error) => reply.send(error).unwrap_or_default(),
        }
    }

    fn chat_worker_order(
        &self,
        env: &EnvironmentName,
        group: GroupId,
        spawn: ChatSpawn,
    ) -> Result<(TerminalGroup, WorkerOrder), Response> {
        let text = spawn.task;
        let preset = spawn.agent;
        let group = self
            .envs
            .get(env)
            .and_then(|live| live.data.group(group))
            .cloned()
            .ok_or_else(|| Response::Error(NO_CHAT_GROUP.to_owned()))?;
        let order = WorkerOrder {
            preset,
            worktree: spawn.worktree,
            brief: chat_worker_brief(&text),
            lead: spawn.lead.map(|session| WorkerOf {
                session,
                task: text,
            }),
        };
        Ok((group, order))
    }
}
