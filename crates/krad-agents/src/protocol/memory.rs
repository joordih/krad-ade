use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallView {
    #[default]
    List,
    Show,
}

impl RecallView {
    #[must_use]
    pub const fn is_list(&self) -> bool {
        matches!(self, Self::List)
    }
}

const MIN_MEMORY_HITS: usize = 1;
pub const DEFAULT_MEMORY_HITS: usize = 8;
pub const MAX_MEMORY_HITS: usize = 30;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryHitInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub excerpt: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEntryInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KraddieChatInfo {
    pub kraddie: String,
    pub session: u64,
    pub terminal: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestInfo {
    pub id: String,
    pub chat: String,
    pub facts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KraddieRecallInfo {
    pub kraddie: String,
    pub keepsakes: Vec<String>,
    pub digests: Vec<DigestInfo>,
    pub reads: Vec<String>,
    pub automatic_digests: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KraddieSharingInfo {
    pub kraddie: String,
    pub reads: Vec<String>,
}

#[must_use]
pub fn memory_hit_limit(asked: Option<usize>) -> usize {
    asked
        .unwrap_or(DEFAULT_MEMORY_HITS)
        .clamp(MIN_MEMORY_HITS, MAX_MEMORY_HITS)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryTools {
    On,
    #[default]
    Off,
}
