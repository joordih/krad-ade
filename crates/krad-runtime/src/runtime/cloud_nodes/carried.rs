use krad_chat::LiveSession;
use krad_domain::{EnvironmentName, GroupId, NodeEdit, NodePlace, SessionId, TerminalId};
use krad_protocol::{CarriedConversation, CloudChatStart};

use super::super::working::agent;
use super::super::Runtime;
use crate::chats::{ChatBrief, ChatRole, ChatStart};
use crate::workers::worker_node;

impl Runtime {
    pub(in crate::runtime) fn open_carried_chats(
        &mut self,
        (environment, session, group): (&EnvironmentName, SessionId, GroupId),
        starts: &[CloudChatStart],
    ) {
        for start in starts {
            if self
                .open_carried_chat((environment, session, group), start)
                .is_none()
            {
                tracing::warn!(session = session.as_u64(), "a carried chat could not open");
            }
        }
    }

    fn open_carried_chat(
        &mut self,
        (environment, session, group): (&EnvironmentName, SessionId, GroupId),
        start: &CloudChatStart,
    ) -> Option<TerminalId> {
        let terminal = TerminalId(self.environments.data(environment)?.next_id);
        let label = self.settings.current.acp.harness_name(start.harness);
        let node = worker_node(terminal, &label, start.harness, Some(group));
        let placed = NodeEdit::Open {
            node: Box::new(node),
            place: NodePlace::Tab { anchor: None },
        };
        self.environments
            .edit(environment.clone(), &placed.into(), None, &mut self.outbox);
        let in_session = self
            .environments
            .data(environment)
            .and_then(|data| data.session_of(terminal))
            == Some(session);
        if !in_session {
            return None;
        }
        let conversation = match &start.conversation {
            CarriedConversation::Fresh(resume) => LiveSession::Start(resume.clone()),
            CarriedConversation::Forked(resume) => LiveSession::Fork(resume.clone()),
        };
        let chat = ChatStart {
            harness: start.harness,
            brief: ChatBrief {
                role: ChatRole::Agent,
                ..ChatBrief::default()
            },
            prepared: None,
            session: conversation,
            messages: Vec::new(),
            queued: start.prompts.clone(),
            notices: Vec::new(),
        };
        self.open_chat(&agent(environment, terminal), chat);
        Some(terminal)
    }
}
