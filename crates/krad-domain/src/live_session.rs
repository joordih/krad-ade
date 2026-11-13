use serde::{Deserialize, Serialize};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{ResumeId, RewindId};

const UUID_VERSION_MASK: u128 = 0xffff_ffff_ffff_0fff_3fff_ffff_ffff_ffff;
const UUID_VERSION_BITS: u128 = 0x0000_0000_0000_4000_8000_0000_0000_0000;
const UUID_GROUP_ENDS: [usize; 4] = [8, 12, 16, 20];
const UUID_HEX_DIGITS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "from", rename_all = "snake_case")]
pub enum LiveSession {
    Start(ResumeId),
    Resume(ResumeId),
    Fork(ResumeId),
    Cut(ResumeCut),
}

impl LiveSession {
    #[must_use]
    pub const fn history(&self) -> Option<&ResumeId> {
        match self {
            Self::Start(_) => None,
            Self::Resume(id) | Self::Fork(id) | Self::Cut(ResumeCut { from: id, .. }) => Some(id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeCut {
    pub from: ResumeId,
    pub at: RewindId,
    pub drops: Option<RewindId>,
}

#[must_use]
pub fn new_session_id() -> ResumeId {
    ResumeId::try_from(new_uuid()).unwrap_or_else(|_| unreachable!("a uuid is a valid resume id"))
}

#[must_use]
pub fn new_uuid() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or_default(|elapsed| elapsed.as_nanos());
    let halves = [RandomState::new(), RandomState::new()].map(|state| {
        let mut hasher = state.build_hasher();
        std::process::id().hash(&mut hasher);
        nanos.hash(&mut hasher);
        u128::from(hasher.finish())
    });
    let bits = ((halves[0] << u64::BITS) | halves[1]) & UUID_VERSION_MASK | UUID_VERSION_BITS;
    let hex = format!("{bits:0UUID_HEX_DIGITS$x}");
    let mut uuid = String::with_capacity(UUID_HEX_DIGITS + UUID_GROUP_ENDS.len());
    for (at, digit) in hex.chars().enumerate() {
        if UUID_GROUP_ENDS.contains(&at) {
            uuid.push('-');
        }
        uuid.push(digit);
    }
    uuid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_intent_round_trips_through_json() -> serde_json::Result<()> {
        let from = new_session_id();
        let sessions = [
            LiveSession::Start(from.clone()),
            LiveSession::Resume(from.clone()),
            LiveSession::Fork(from.clone()),
            LiveSession::Cut(ResumeCut {
                from,
                at: RewindId::from(new_uuid()),
                drops: None,
            }),
        ];
        for session in sessions {
            let json = serde_json::to_string(&session)?;
            assert_eq!(serde_json::from_str::<LiveSession>(&json)?, session);
        }
        Ok(())
    }
}
