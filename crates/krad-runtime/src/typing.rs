use krad_protocol::AgentRef;
use std::time::{Duration, Instant};

pub(crate) const ENTER: &[u8] = b"\r";
const PROMPT_DELAY: Duration = Duration::from_secs(4);
const NOTICE_ENTER_DELAY: Duration = Duration::from_millis(500);
const KEY_STEP_DELAY: Duration = Duration::from_secs(1);
const ENTER_RETRY_DELAY: Duration = Duration::from_secs(2);
const ENTER_RETRIES: u8 = 3;
const BRACKETED_PASTE_START: &[u8] = b"\x1b[200~";
const BRACKETED_PASTE_END: &[u8] = b"\x1b[201~";

#[must_use]
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    if !bracketed {
        return text.as_bytes().to_vec();
    }
    [BRACKETED_PASTE_START, text.as_bytes(), BRACKETED_PASTE_END].concat()
}

#[derive(Debug)]
struct Prompt {
    target: AgentRef,
    text: String,
    due: Instant,
}

#[derive(Debug)]
struct Step {
    target: AgentRef,
    keys: Vec<u8>,
    due: Instant,
}

#[derive(Debug)]
pub(crate) struct Enter {
    pub(crate) target: AgentRef,
    due: Instant,
    retries_left: u8,
    pub(crate) is_retry: bool,
}

impl Enter {
    fn new(target: AgentRef, now: Instant) -> Self {
        Self {
            target,
            due: now + NOTICE_ENTER_DELAY,
            retries_left: ENTER_RETRIES,
            is_retry: false,
        }
    }

    pub(crate) fn retry(self, now: Instant) -> Option<Self> {
        Some(Self {
            due: now + ENTER_RETRY_DELAY,
            retries_left: self.retries_left.checked_sub(1)?,
            is_retry: true,
            target: self.target,
        })
    }
}

#[derive(Debug)]
pub(crate) enum Stroke {
    Paste { target: AgentRef, text: String },
    Keys { target: AgentRef, keys: Vec<u8> },
    Enter(Enter),
}

#[derive(Debug, Default)]
pub(crate) struct Typist {
    prompts: Vec<Prompt>,
    steps: Vec<Step>,
    enters: Vec<Enter>,
}

fn take_due<T>(items: &mut Vec<T>, now: Instant, due: impl Fn(&T) -> Instant) -> Vec<T> {
    let (ready, later) = std::mem::take(items)
        .into_iter()
        .partition(|item| due(item) <= now);
    *items = later;
    ready
}

impl Typist {
    pub(crate) fn prompt(&mut self, target: AgentRef, text: String, now: Instant) {
        self.prompts.push(Prompt {
            target,
            text,
            due: now + PROMPT_DELAY,
        });
    }

    pub(crate) fn confirm_enter(&mut self, target: AgentRef, now: Instant) {
        self.enters.push(Enter::new(target, now));
    }

    pub(crate) fn later_steps(
        &mut self,
        target: &AgentRef,
        steps: impl IntoIterator<Item = Vec<u8>>,
        now: Instant,
    ) {
        let mut due = now;
        for keys in steps {
            due += KEY_STEP_DELAY;
            self.steps.push(Step {
                target: target.clone(),
                keys,
                due,
            });
        }
    }

    pub(crate) fn retry(&mut self, enter: Enter, now: Instant) {
        self.enters.extend(enter.retry(now));
    }

    pub(crate) fn is_starting(&self, target: &AgentRef) -> bool {
        self.prompts.iter().any(|prompt| prompt.target == *target)
            || self.enters.iter().any(|enter| enter.target == *target)
    }

    pub(crate) fn forget(&mut self, target: &AgentRef) {
        self.prompts.retain(|prompt| prompt.target != *target);
        self.steps.retain(|step| step.target != *target);
        self.enters.retain(|enter| enter.target != *target);
    }

    pub(crate) fn due(&mut self, now: Instant) -> Vec<Stroke> {
        let keys = take_due(&mut self.steps, now, |step| step.due)
            .into_iter()
            .map(|step| Stroke::Keys {
                target: step.target,
                keys: step.keys,
            });
        let enters = take_due(&mut self.enters, now, |enter| enter.due)
            .into_iter()
            .map(Stroke::Enter);
        let prompts = take_due(&mut self.prompts, now, |prompt| prompt.due)
            .into_iter()
            .map(|prompt| Stroke::Paste {
                target: prompt.target,
                text: prompt.text,
            });
        keys.chain(enters).chain(prompts).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{EnvironmentName, TerminalId};

    fn target() -> AgentRef {
        AgentRef {
            environment: EnvironmentName::new("rag".to_owned()),
            terminal: TerminalId(7),
        }
    }

    #[test]
    fn an_unconfirmed_enter_is_retried_a_bounded_number_of_times() {
        let now = Instant::now();
        let mut enter = Some(Enter::new(target(), now));
        let mut presses = 0;
        while let Some(pending) = enter {
            presses += 1;
            enter = pending.retry(now);
            assert!(enter.as_ref().is_none_or(|next| next.is_retry));
        }
        assert_eq!(presses, 1 + usize::from(ENTER_RETRIES));
    }

    #[test]
    fn a_prompt_waits_for_its_delay_and_keeps_its_target_starting() {
        let now = Instant::now();
        let mut typist = Typist::default();
        typist.prompt(target(), "fix it".to_owned(), now);
        assert!(typist.is_starting(&target()));
        assert!(typist.due(now).is_empty());
        let strokes = typist.due(now + PROMPT_DELAY);
        assert!(matches!(
            strokes.as_slice(),
            [Stroke::Paste { text, .. }] if text == "fix it"
        ));
        assert!(!typist.is_starting(&target()));
    }

    #[test]
    fn later_steps_come_a_second_apart_in_order() {
        let now = Instant::now();
        let mut typist = Typist::default();
        typist.later_steps(&target(), [b"a".to_vec(), b"b".to_vec()], now);
        assert!(typist.due(now).is_empty());
        let first = typist.due(now + KEY_STEP_DELAY);
        assert!(matches!(first.as_slice(), [Stroke::Keys { keys, .. }] if keys == b"a"));
        let second = typist.due(now + KEY_STEP_DELAY * 2);
        assert!(matches!(second.as_slice(), [Stroke::Keys { keys, .. }] if keys == b"b"));
    }

    #[test]
    fn a_forgotten_target_types_nothing_more() {
        let now = Instant::now();
        let mut typist = Typist::default();
        typist.prompt(target(), "fix it".to_owned(), now);
        typist.confirm_enter(target(), now);
        typist.forget(&target());
        assert!(typist.due(now + PROMPT_DELAY).is_empty());
    }

    #[test]
    fn pasting_wraps_the_text_only_when_the_terminal_asks_for_it() {
        assert_eq!(paste_bytes("fix it", false), b"fix it");
        assert_eq!(paste_bytes("fix it", true), b"\x1b[200~fix it\x1b[201~");
    }
}
