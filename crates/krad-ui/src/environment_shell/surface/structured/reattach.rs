use gpui_kit::Context;
use krad_chat::{AttachRefusal, ChatEvent, CloudRun, LiveEvent};
use krad_domain::{EnvironmentName, TerminalId};

use super::super::super::EnvironmentShell;
use super::{structured_mut, Batch, StructuredChat};

const TRIMMED_TEXT: &str =
    "The cloud runner trimmed this chat's journal, so it attached again from its start";

const fn ends_attach(event: &LiveEvent) -> bool {
    matches!(
        event,
        LiveEvent::Chat(ChatEvent::Error(_) | ChatEvent::Done)
    )
}

const fn past_end(refusal: Option<AttachRefusal>) -> bool {
    matches!(refusal, Some(AttachRefusal::PastEnd { .. }))
}

impl StructuredChat {
    fn reattaches(&mut self) -> Batch {
        self.live = None;
        self.outstanding = 0;
        self.working = false;
        self.last_batch.take().unwrap_or_default()
    }
}

impl EnvironmentShell {
    pub(super) fn reattach_past_end(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        event: &LiveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !ends_attach(event)
            || !past_end(self.attached_run(env, id).and_then(CloudRun::refusal))
            || !self.rewind_cloud_attach(env, id)
        {
            return false;
        }
        let Some(structured) = structured_mut(self, env, id) else {
            return false;
        };
        let batch = structured.reattaches();
        tracing::info!(
            terminal = id.as_u64(),
            "attaching to the cloud chat again from the journal's start"
        );
        self.hand_over_again(
            env,
            id,
            batch,
            ChatEvent::Error(TRIMMED_TEXT.to_owned()),
            cx,
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::JournalOffset;

    #[test]
    fn only_an_ended_attach_past_the_journal_end_attaches_again() {
        let trimmed = Some(AttachRefusal::PastEnd {
            end: JournalOffset(10),
        });
        assert!(past_end(trimmed));
        assert!(!past_end(Some(AttachRefusal::Refused)));
        assert!(!past_end(None));
        assert!(ends_attach(&LiveEvent::Chat(ChatEvent::Done)));
        assert!(!ends_attach(&LiveEvent::Notice(String::new())));
    }
}
