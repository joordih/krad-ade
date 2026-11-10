use super::owner::DigestId;
use crate::ids::{EnvironmentName, ResumeId, SessionId, TerminalId};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::time::SystemTime;

pub const MAX_DIGEST_FACTS: usize = 12;
pub const MAX_DIGEST_FACT_CHARS: usize = 300;
pub const MAX_DIGESTS: usize = 400;
const FACT_LINE: &str = "\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigestError {
    Empty,
    NoSuchDigest,
}

impl fmt::Display for DigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "A chat digest needs at least one fact",
            Self::NoSuchDigest => "No chat digest there",
        })
    }
}

impl std::error::Error for DigestError {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DigestChat {
    pub environment: EnvironmentName,
    pub session: SessionId,
    pub terminal: TerminalId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<ResumeId>,
}

impl DigestChat {
    fn same_chat(&self, other: &Self) -> bool {
        self.environment == other.environment && self.terminal == other.terminal
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DigestAuthor {
    #[default]
    Harness,
    User,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatDigest {
    pub id: DigestId,
    pub chat: DigestChat,
    pub written: SystemTime,
    pub facts: Vec<String>,
    #[serde(default)]
    pub covered: usize,
    #[serde(default)]
    pub author: DigestAuthor,
}

impl ChatDigest {
    #[must_use]
    pub fn text(&self) -> String {
        self.facts.join(FACT_LINE)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DigestDraft {
    pub chat: DigestChat,
    pub facts: Vec<String>,
    pub covered: usize,
    pub written: SystemTime,
}

#[must_use]
pub fn bounded_fact(text: &str) -> Option<String> {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return None;
    }
    Some(line.chars().take(MAX_DIGEST_FACT_CHARS).collect())
}

fn bounded_facts<'a>(facts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for fact in facts.into_iter().filter_map(bounded_fact) {
        if kept.len() >= MAX_DIGEST_FACTS {
            break;
        }
        if !kept.contains(&fact) {
            kept.push(fact);
        }
    }
    kept
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DigestBook {
    digests: Vec<ChatDigest>,
}

impl DigestBook {
    #[must_use]
    pub fn new(digests: Vec<ChatDigest>) -> Self {
        let mut book = Self::default();
        for digest in digests {
            let facts = bounded_facts(digest.facts.iter().map(String::as_str));
            let taken = book.digests.iter().any(|kept| kept.id == digest.id);
            if facts.is_empty() || taken {
                continue;
            }
            book.digests.push(ChatDigest { facts, ..digest });
        }
        book.digests.sort_by_key(|digest| digest.written);
        book.trim();
        book
    }

    #[must_use]
    pub fn digests(&self) -> &[ChatDigest] {
        &self.digests
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.digests.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.digests.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: DigestId) -> Option<&ChatDigest> {
        self.digests.iter().find(|digest| digest.id == id)
    }

    #[must_use]
    pub fn of_chat(&self, chat: &DigestChat) -> Option<&ChatDigest> {
        self.digests
            .iter()
            .find(|digest| digest.chat.same_chat(chat))
    }

    fn next_id(&self) -> DigestId {
        DigestId(
            self.digests
                .iter()
                .map(|digest| digest.id.0)
                .max()
                .map_or(1, |last| last + 1),
        )
    }

    pub fn record(&mut self, draft: DigestDraft) -> Option<DigestId> {
        let fresh = bounded_facts(draft.facts.iter().map(String::as_str));
        if fresh.is_empty() {
            return None;
        }
        if let Some(at) = self
            .digests
            .iter()
            .position(|digest| digest.chat.same_chat(&draft.chat))
        {
            let mut digest = self.digests.remove(at);
            digest.facts = match digest.author {
                DigestAuthor::User => {
                    bounded_facts(digest.facts.iter().chain(&fresh).map(String::as_str))
                }
                DigestAuthor::Harness => fresh,
            };
            digest.chat = draft.chat;
            digest.covered = draft.covered;
            digest.written = draft.written;
            let id = digest.id;
            self.digests.push(digest);
            return Some(id);
        }
        let id = self.next_id();
        self.digests.push(ChatDigest {
            id,
            chat: draft.chat,
            written: draft.written,
            facts: fresh,
            covered: draft.covered,
            author: DigestAuthor::Harness,
        });
        self.trim();
        Some(id)
    }

    /// # Errors
    /// Refuses an unknown digest or text without any fact.
    pub fn edit(&mut self, id: DigestId, text: &str) -> Result<(), DigestError> {
        let facts = bounded_facts(text.lines());
        if facts.is_empty() {
            return Err(DigestError::Empty);
        }
        let digest = self
            .digests
            .iter_mut()
            .find(|digest| digest.id == id)
            .ok_or(DigestError::NoSuchDigest)?;
        digest.facts = facts;
        digest.author = DigestAuthor::User;
        Ok(())
    }

    /// # Errors
    /// Refuses an unknown digest.
    pub fn remove(&mut self, id: DigestId) -> Result<ChatDigest, DigestError> {
        let at = self
            .digests
            .iter()
            .position(|digest| digest.id == id)
            .ok_or(DigestError::NoSuchDigest)?;
        Ok(self.digests.remove(at))
    }

    fn trim(&mut self) {
        let extra = self.digests.len().saturating_sub(MAX_DIGESTS);
        self.digests.drain(..extra);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn chat(terminal: u64) -> DigestChat {
        DigestChat {
            environment: EnvironmentName::new("main".to_owned()),
            session: SessionId(1),
            terminal: TerminalId(terminal),
            title: format!("Chat {terminal}"),
            folder: None,
            resume: None,
        }
    }

    fn draft(terminal: u64, facts: &[&str], seconds: u64) -> DigestDraft {
        DigestDraft {
            chat: chat(terminal),
            facts: facts.iter().map(|fact| (*fact).to_owned()).collect(),
            covered: facts.len(),
            written: SystemTime::UNIX_EPOCH + Duration::from_secs(seconds),
        }
    }

    #[test]
    fn a_chat_keeps_one_digest_that_a_later_one_replaces() {
        let mut book = DigestBook::default();
        assert_eq!(book.record(draft(1, &["Uses sqlx"], 10)), Some(DigestId(1)));
        assert_eq!(book.record(draft(2, &["Port 8080"], 20)), Some(DigestId(2)));
        assert_eq!(
            book.record(draft(1, &["Uses sqlx", "Moved to diesel"], 30)),
            Some(DigestId(1))
        );
        assert_eq!(book.len(), 2);
        assert_eq!(
            book.of_chat(&chat(1)).map(|digest| digest.facts.clone()),
            Some(vec!["Uses sqlx".to_owned(), "Moved to diesel".to_owned()])
        );
        assert_eq!(book.record(draft(3, &["  "], 40)), None);
    }

    #[test]
    fn the_users_edit_survives_a_later_digest_of_the_same_chat() {
        let mut book = DigestBook::default();
        let id = book.record(draft(1, &["Guess"], 10));
        let id = id.unwrap_or_else(|| unreachable!());
        assert_eq!(book.edit(id, "Exact fact\n\n  second  fact "), Ok(()));
        assert_eq!(book.edit(id, "  \n"), Err(DigestError::Empty));
        assert_eq!(book.edit(DigestId(9), "x"), Err(DigestError::NoSuchDigest));
        book.record(draft(1, &["New finding", "Exact fact"], 20));
        assert_eq!(
            book.get(id).map(|digest| digest.facts.clone()),
            Some(vec![
                "Exact fact".to_owned(),
                "second fact".to_owned(),
                "New finding".to_owned()
            ])
        );
        assert!(book.remove(id).is_ok());
        assert_eq!(book.remove(id), Err(DigestError::NoSuchDigest));
    }

    #[test]
    fn facts_and_digests_are_bounded() {
        let many: Vec<String> = (0..MAX_DIGEST_FACTS + 4).map(|n| format!("f{n}")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        let mut book = DigestBook::default();
        book.record(draft(1, &many, 1));
        assert_eq!(book.digests()[0].facts.len(), MAX_DIGEST_FACTS);
        let long = "x".repeat(MAX_DIGEST_FACT_CHARS * 2);
        book.record(draft(2, &[&long], 2));
        assert_eq!(
            book.digests()[1].facts[0].chars().count(),
            MAX_DIGEST_FACT_CHARS
        );
        for terminal in 3..(3 + MAX_DIGESTS as u64) {
            book.record(draft(terminal, &["fact"], terminal));
        }
        assert_eq!(book.len(), MAX_DIGESTS);
        assert!(book.of_chat(&chat(1)).is_none(), "the oldest went first");
    }
}
