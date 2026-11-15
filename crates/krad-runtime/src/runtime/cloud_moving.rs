use krad_domain::{CloudPlacement, EnvironmentName, NodeEdit, NodeField, SessionEdit, SessionId};
use krad_protocol::{AgentRef, CloudNews};

use super::cloud_nodes::carried_start;
use super::working::agent;
use super::Runtime;
use crate::cloud::MovedChat;

impl Runtime {
    #[tracing::instrument(skip_all, fields(session = session.as_u64(), runner = %placement.runner))]
    pub(super) fn cloud_moved(
        &mut self,
        (environment, session): (EnvironmentName, SessionId),
        placement: CloudPlacement,
        moved: Vec<MovedChat>,
    ) {
        let still_here = self
            .environments
            .data(&environment)
            .and_then(|data| data.session(session))
            .is_some_and(|found| found.cloud.is_none());
        if !still_here {
            tracing::info!("the session closed or moved before the runner was ready");
            return self.cloud_news(CloudNews::NoSession);
        }
        let placed = SessionEdit::SetCloud {
            session,
            cloud: Some(placement),
        };
        self.environments
            .edit(environment.clone(), &placed.into(), None, &mut self.outbox);
        for chat in moved {
            let id = agent(&environment, chat.terminal);
            self.move_chat(&id, chat);
        }
        tracing::info!("the session now runs on its cloud runner");
    }

    fn move_chat(&mut self, id: &AgentRef, moved: MovedChat) {
        let harness = moved.start.harness;
        let changed = self
            .environments
            .data(&id.environment)
            .and_then(|data| data.nodes.iter().find(|node| node.id == id.terminal))
            .is_some_and(|node| node.harness != Some(harness));
        if changed {
            let edit = NodeEdit::Set {
                terminal: id.terminal,
                field: NodeField::Harness(Some(harness)),
            };
            self.environments
                .edit(id.environment.clone(), &edit.into(), None, &mut self.outbox);
        }
        let start = carried_start(moved.start);
        self.with_chat_work(|chats, work| chats.move_in_place(id, start, work));
    }
}
