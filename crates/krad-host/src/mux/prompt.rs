use krad_domain::SshDestination;
use std::fmt;
use std::sync::mpsc;

const CONFIRM_MARKER: &str = "(yes/no";
const REDACTED: &str = "[REDACTED]";

#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    #[must_use]
    pub const fn new(text: String) -> Self {
        Self(text)
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptKind {
    Secret,
    Confirm,
}

impl PromptKind {
    #[must_use]
    pub fn of(text: &str) -> Self {
        if text.contains(CONFIRM_MARKER) {
            Self::Confirm
        } else {
            Self::Secret
        }
    }
}

#[derive(Debug)]
pub struct SignInPrompt {
    pub destination: SshDestination,
    pub text: String,
    pub kind: PromptKind,
    answer: mpsc::Sender<Option<Secret>>,
}

impl SignInPrompt {
    pub(crate) fn new(
        destination: SshDestination,
        text: String,
        answer: mpsc::Sender<Option<Secret>>,
    ) -> Self {
        Self {
            destination,
            kind: PromptKind::of(&text),
            text,
            answer,
        }
    }

    pub fn answer(self, answer: Option<Secret>) {
        self.answer.send(answer).ok();
    }
}

pub type PromptSender = mpsc::Sender<SignInPrompt>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_key_questions_are_confirmations_and_the_rest_secrets() {
        assert_eq!(
            PromptKind::of("Are you sure you want to continue connecting (yes/no/[fingerprint])? "),
            PromptKind::Confirm
        );
        assert_eq!(PromptKind::of("me@devbox's password: "), PromptKind::Secret);
    }

    #[test]
    fn secrets_never_print() {
        assert_eq!(format!("{:?}", Secret::new("hunter2".to_owned())), REDACTED);
    }
}
