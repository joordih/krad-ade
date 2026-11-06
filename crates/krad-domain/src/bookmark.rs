use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const PRINT_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRINT_PRIME: u64 = 0x0100_0000_01b3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TurnPrint(u64);

impl Default for TurnPrint {
    fn default() -> Self {
        Self(PRINT_OFFSET)
    }
}

impl TurnPrint {
    #[must_use]
    pub fn and(self, part: &[u8]) -> Self {
        Self(part.iter().fold(self.0, |print, byte| {
            (print ^ u64::from(*byte)).wrapping_mul(PRINT_PRIME)
        }))
    }
}

impl From<&str> for TurnPrint {
    fn from(question: &str) -> Self {
        Self::default().and(question.trim().as_bytes())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TurnSide {
    Question,
    Answer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Bookmark {
    pub turn: TurnPrint,
    #[serde(default)]
    pub repeat: u32,
    pub side: TurnSide,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChatBookmarks(BTreeSet<Bookmark>);

impl ChatBookmarks {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn contains(&self, bookmark: &Bookmark) -> bool {
        self.0.contains(bookmark)
    }

    pub fn toggle(&mut self, bookmark: Bookmark) {
        if !self.0.remove(&bookmark) {
            self.0.insert(bookmark);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer_to(question: &str) -> Bookmark {
        Bookmark {
            turn: TurnPrint::from(question),
            repeat: 0,
            side: TurnSide::Answer,
        }
    }

    #[test]
    fn a_turn_is_known_by_its_question_whatever_the_surrounding_space() {
        assert_eq!(
            TurnPrint::from("  Fix the build\n"),
            TurnPrint::from("Fix the build")
        );
        assert_ne!(
            TurnPrint::from("Fix the build"),
            TurnPrint::from("Fix the tests")
        );
    }

    #[test]
    fn a_bookmark_written_before_repeats_were_counted_points_at_the_first_ask(
    ) -> serde_json::Result<()> {
        let older = serde_json::json!({ "turn": 42, "side": "Answer" });
        let bookmark: Bookmark = serde_json::from_value(older)?;
        assert_eq!(bookmark.repeat, 0);
        Ok(())
    }

    #[test]
    fn toggling_a_bookmark_adds_it_then_removes_it() {
        let mut bookmarks = ChatBookmarks::default();
        bookmarks.toggle(answer_to("Fix the build"));
        assert!(bookmarks.contains(&answer_to("Fix the build")));
        assert!(!bookmarks.contains(&answer_to("Fix the tests")));
        bookmarks.toggle(answer_to("Fix the build"));
        assert!(bookmarks.is_empty());
    }
}
