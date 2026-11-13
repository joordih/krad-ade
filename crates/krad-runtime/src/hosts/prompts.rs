use krad_host::mux::{PromptSender, Secret, SignInPrompt};
use krad_protocol::{ErrorCode, EventKind, HostPrompt, PromptAnswer, PromptId, Reply};
use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver};

use crate::runtime::refused;

const UNKNOWN_PROMPT: &str = "no sign-in prompt of that id is waiting";

#[derive(Debug)]
pub(crate) struct Prompts {
    arriving: Receiver<SignInPrompt>,
    waiting: BTreeMap<PromptId, SignInPrompt>,
    last: PromptId,
}

impl Prompts {
    pub(crate) fn open() -> (Self, PromptSender) {
        let (sender, arriving) = channel();
        let prompts = Self {
            arriving,
            waiting: BTreeMap::new(),
            last: PromptId::default(),
        };
        (prompts, sender)
    }

    pub(crate) fn collect(&mut self, outbox: &mut Vec<EventKind>) {
        while let Ok(prompt) = self.arriving.try_recv() {
            self.last = self.last.next();
            outbox.push(EventKind::HostPrompted(shown(self.last, &prompt)));
            self.waiting.insert(self.last, prompt);
        }
    }

    pub(crate) fn answer(
        &mut self,
        prompt: PromptId,
        answer: Option<PromptAnswer>,
        outbox: &mut Vec<EventKind>,
    ) -> Reply {
        let Some(waiting) = self.waiting.remove(&prompt) else {
            return refused(ErrorCode::NotFound, UNKNOWN_PROMPT);
        };
        waiting.answer(answer.map(|answer| Secret::new(answer.into_text())));
        outbox.push(EventKind::HostPromptSettled { prompt });
        Reply::Accepted
    }

    pub(crate) fn snapshot(&self) -> Vec<HostPrompt> {
        self.waiting
            .iter()
            .map(|(id, prompt)| shown(*id, prompt))
            .collect()
    }
}

fn shown(prompt: PromptId, waiting: &SignInPrompt) -> HostPrompt {
    HostPrompt {
        prompt,
        asker: waiting.asker.clone(),
        kind: waiting.kind,
        text: waiting.text.clone(),
    }
}
