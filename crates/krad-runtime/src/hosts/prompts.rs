use krad_domain::PromptAsker;
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
    declined: Vec<PromptAsker>,
    last: PromptId,
}

impl Prompts {
    pub(crate) fn open() -> (Self, PromptSender) {
        let (sender, arriving) = channel();
        let prompts = Self {
            arriving,
            waiting: BTreeMap::new(),
            declined: Vec::new(),
            last: PromptId::default(),
        };
        (prompts, sender)
    }

    pub(crate) fn collect(&mut self, outbox: &mut Vec<EventKind>) {
        while let Ok(prompt) = self.arriving.try_recv() {
            if self.declined.contains(&prompt.asker) {
                prompt.answer(None);
                continue;
            }
            self.last = self.last.next();
            outbox.push(EventKind::HostPrompted(shown(self.last, &prompt)));
            self.waiting.insert(self.last, prompt);
        }
    }

    pub(crate) fn asker(&self, prompt: PromptId) -> Option<&PromptAsker> {
        self.waiting.get(&prompt).map(|waiting| &waiting.asker)
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

    pub(crate) fn decline(&mut self, asker: &PromptAsker, outbox: &mut Vec<EventKind>) {
        if !self.declined.contains(asker) {
            self.declined.push(asker.clone());
        }
        let settled: Vec<PromptId> = self
            .waiting
            .iter()
            .filter(|(_, waiting)| waiting.asker == *asker)
            .map(|(id, _)| *id)
            .collect();
        for prompt in settled {
            self.answer(prompt, None, outbox);
        }
    }

    pub(crate) fn admit(&mut self, asker: &PromptAsker) {
        self.declined.retain(|declined| declined != asker);
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

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::CloudRunnerId;
    use std::sync::mpsc;

    fn runner() -> PromptAsker {
        PromptAsker::Runner {
            runner: CloudRunnerId(2),
        }
    }

    fn ask(sender: &PromptSender, asker: PromptAsker) -> mpsc::Receiver<Option<Secret>> {
        let (answer, answered) = mpsc::channel();
        drop(sender.send(SignInPrompt::new(
            asker,
            "me@gpu's password: ".to_owned(),
            answer,
        )));
        answered
    }

    #[test]
    fn a_declined_asker_is_answered_at_once_until_it_is_admitted_again(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut prompts, sender) = Prompts::open();
        let mut outbox = Vec::new();
        let first = ask(&sender, runner());
        let git = ask(&sender, PromptAsker::Git);
        prompts.collect(&mut outbox);
        assert_eq!(prompts.snapshot().len(), 2);
        prompts.decline(&runner(), &mut outbox);
        assert_eq!(first.try_recv()?, None);
        assert!(git.try_recv().is_err());
        let left: Vec<PromptAsker> = prompts
            .snapshot()
            .into_iter()
            .map(|prompt| prompt.asker)
            .collect();
        assert_eq!(left, [PromptAsker::Git]);
        let again = ask(&sender, runner());
        prompts.collect(&mut outbox);
        assert_eq!(again.try_recv()?, None);
        prompts.admit(&runner());
        let admitted = ask(&sender, runner());
        prompts.collect(&mut outbox);
        assert!(admitted.try_recv().is_err());
        assert_eq!(prompts.snapshot().len(), 2);
        Ok(())
    }
}
