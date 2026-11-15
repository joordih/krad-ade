use krad_protocol::AgentRef;

use super::{ChatStart, ChatWork, Chats};

impl Chats {
    #[tracing::instrument(skip_all, fields(environment = id.environment.as_str(), chat = id.terminal.0))]
    pub(crate) fn move_in_place(
        &mut self,
        id: &AgentRef,
        mut start: ChatStart,
        work: &mut ChatWork<'_>,
    ) {
        if let Some(mut left) = self.open.remove(id) {
            if let Some(live) = &left.live {
                live.stop();
            }
            start.messages = std::mem::take(&mut left.messages);
            if left.harness == start.harness {
                start.brief.model = left.choices.model;
                start.brief.effort = left.choices.effort;
                start.brief.permission_mode = left.choices.permission_mode;
            }
            tracing::info!("the chat's local process stopped so it goes on in the cloud");
        }
        self.open(id, start, work);
    }
}
