use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::Context;
use krad_agents::{ChatHandle, LeadHandle, Request, Response};
use krad_domain::{EnvironmentName, SessionId};

use super::super::EnvironmentShell;
use super::chat::ChatSpawn;

const NOT_A_LEAD_ANYMORE: &str = "This chat is no longer its session's lead";
const NO_LEAD_GROUP: &str = "The lead's session belongs to no group, so it cannot start workers";

impl EnvironmentShell {
    pub(super) fn answer_lead(
        &mut self,
        lead: &LeadHandle,
        request: Request,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let session = SessionId(lead.session);
        let group = self
            .envs
            .get(&EnvironmentName::new(lead.environment.clone()))
            .and_then(|live| live.data.session(session))
            .filter(|session| session.lead.is_some())
            .map(|session| session.group);
        let refusal = match group {
            None => NOT_A_LEAD_ANYMORE,
            Some(None) => NO_LEAD_GROUP,
            Some(Some(group)) => {
                let chat = ChatHandle {
                    environment: lead.environment.clone(),
                    group: group.as_u64(),
                };
                match request {
                    Request::Spawn {
                        agent,
                        worktree,
                        task,
                    } => {
                        let spawn = ChatSpawn {
                            agent,
                            worktree,
                            task,
                            lead: Some(session),
                        };
                        self.start_chat_worker(&chat, spawn, reply, cx);
                    }
                    other => self.answer_chat(&chat, other, reply, cx),
                }
                return;
            }
        };
        reply
            .send(Response::Error(refusal.to_owned()))
            .unwrap_or_default();
    }
}
