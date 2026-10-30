use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::Context;
use krad_agents::{ChatHandle, Request, Response};
use krad_domain::{
    chat_worker_brief, EnvironmentName, GroupId, Route, SessionId, TerminalGroup, TerminalId,
};

use super::super::agent_hooks::{preset_named, WorkerOf, WorkerOrder, UNKNOWN_AGENT};
use super::super::EnvironmentShell;
use super::requests::CLOSED_TERMINAL;

const NO_CHAT_INBOX: &str = "The chat has no inbox: follow agents with wait and read";
const TERMINALS_ONLY: &str = "Only agents in terminals can do that";
const NO_CHAT_GROUP: &str = "The chat's group no longer exists";

pub(super) struct ChatSpawn {
    pub(super) agent: String,
    pub(super) worktree: Option<String>,
    pub(super) task: String,
    pub(super) lead: Option<SessionId>,
}

impl EnvironmentShell {
    pub(super) fn answer_chat(
        &mut self,
        chat: &ChatHandle,
        request: Request,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let environment = chat.environment.as_str();
        let response = match request {
            Request::List => Response::Agents(self.agent_infos(environment, None)),
            Request::Send { to, body } => self.submit_from_chat(environment, to, &body),
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
                self.start_chat_worker(chat, spawn, reply, cx);
                return;
            }
            Request::Check { .. } => Response::Error(NO_CHAT_INBOX.to_owned()),
            Request::Hook { .. } | Request::Status(_) | Request::Ask { .. } => {
                Response::Error(TERMINALS_ONLY.to_owned())
            }
        };
        reply.send(response).unwrap_or_default();
    }

    fn submit_from_chat(&mut self, environment: &str, to: u64, body: &str) -> Response {
        let env = EnvironmentName::new(environment.to_owned());
        let id = TerminalId(to);
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
        chat: &ChatHandle,
        spawn: ChatSpawn,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let env = EnvironmentName::new(chat.environment.clone());
        match self.chat_worker_order(&env, GroupId(chat.group), spawn) {
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
        let preset = preset_named(&spawn.agent)
            .filter(|preset| preset.is_agent())
            .ok_or_else(|| Response::Error(UNKNOWN_AGENT.to_owned()))?;
        let group = self
            .envs
            .get(env)
            .and_then(|live| live.data.group(group))
            .cloned()
            .ok_or_else(|| Response::Error(NO_CHAT_GROUP.to_owned()))?;
        let route = spawn
            .lead
            .and_then(|session| self.lead_route(env, session))
            .unwrap_or_else(|| self.settings.gateway.new_chat_route(preset));
        let order = WorkerOrder {
            preset,
            worktree: spawn.worktree,
            brief: chat_worker_brief(&text),
            lead: spawn.lead.map(|session| WorkerOf {
                session,
                task: text,
            }),
            route,
        };
        Ok((group, order))
    }
    fn lead_route(&self, env: &EnvironmentName, session: SessionId) -> Option<Route> {
        let live = self.envs.get(env)?;
        let lead = live.data.session(session)?.lead.as_ref()?.terminal?;
        Some(self.terminal_route(env, lead))
    }
}
