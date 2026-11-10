use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::{HunkFingerprint, Session};
use crate::environment::EnvironmentData;
use crate::git_object::GitObjectId;
use crate::ids::SessionId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionChanges {
    pub baseline: GitObjectId,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub reviewed: BTreeSet<HunkFingerprint>,
}

impl SessionChanges {
    #[must_use]
    pub fn new(baseline: GitObjectId) -> Self {
        Self {
            baseline,
            reviewed: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn is_reviewed(&self, hunks: &[HunkFingerprint]) -> bool {
        !hunks.is_empty() && hunks.iter().all(|hunk| self.reviewed.contains(hunk))
    }

    pub fn set_reviewed(&mut self, hunks: &[HunkFingerprint], reviewed: bool) {
        for hunk in hunks {
            if reviewed {
                self.reviewed.insert(*hunk);
            } else {
                self.reviewed.remove(hunk);
            }
        }
    }

    pub fn keep_reviewed_of(&mut self, present: &BTreeSet<HunkFingerprint>) {
        self.reviewed.retain(|hunk| present.contains(hunk));
    }
}

impl EnvironmentData {
    pub fn session_mut(&mut self, id: SessionId) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|session| session.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBJECT: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

    #[test]
    fn a_fingerprint_depends_on_every_part_and_where_they_split() {
        let one = HunkFingerprint::new([b"a.rs".as_slice(), b"+x\n".as_slice()]);
        assert_eq!(
            one,
            HunkFingerprint::new([b"a.rs".as_slice(), b"+x\n".as_slice()])
        );
        assert_ne!(
            one,
            HunkFingerprint::new([b"b.rs".as_slice(), b"+x\n".as_slice()])
        );
        assert_ne!(
            HunkFingerprint::new([b"ab".as_slice(), b"c".as_slice()]),
            HunkFingerprint::new([b"a".as_slice(), b"bc".as_slice()])
        );
    }

    #[test]
    fn a_file_is_reviewed_once_all_its_hunks_are_and_survives_a_round_trip(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let first = HunkFingerprint::new([b"1".as_slice()]);
        let second = HunkFingerprint::new([b"2".as_slice()]);
        let mut changes = SessionChanges::new(OBJECT.parse()?);
        changes.set_reviewed(&[first], true);
        assert!(!changes.is_reviewed(&[first, second]));
        changes.set_reviewed(&[second], true);
        assert!(changes.is_reviewed(&[first, second]));
        assert!(!changes.is_reviewed(&[]));
        let restored: SessionChanges = serde_json::from_str(&serde_json::to_string(&changes)?)?;
        assert_eq!(restored, changes);
        changes.keep_reviewed_of(&BTreeSet::from([second]));
        assert!(!changes.is_reviewed(&[first]));
        changes.set_reviewed(&[second], false);
        assert!(changes.reviewed.is_empty());
        Ok(())
    }
}
