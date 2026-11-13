pub use krad_domain::{PromptAsker, PromptKind};
use std::fmt;
use std::sync::mpsc;

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

#[derive(Debug)]
pub struct SignInPrompt {
    pub asker: PromptAsker,
    pub text: String,
    pub kind: PromptKind,
    answer: mpsc::Sender<Option<Secret>>,
}

impl SignInPrompt {
    #[must_use]
    pub fn new(asker: PromptAsker, text: String, answer: mpsc::Sender<Option<Secret>>) -> Self {
        Self {
            asker,
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
    fn secrets_never_print() {
        assert_eq!(format!("{:?}", Secret::new("hunter2".to_owned())), REDACTED);
    }
}
