mod digest;
mod digest_file;
mod library;
mod mention;
mod owner;
mod pack;
mod reach;
mod timing;
mod writing;

pub use digest::{
    bounded_fact, ChatDigest, DigestAuthor, DigestBook, DigestChat, DigestDraft, DigestError,
    MAX_DIGESTS, MAX_DIGEST_FACTS, MAX_DIGEST_FACT_CHARS,
};
pub use digest_file::{
    read_digests, write_digests, DigestsFileError, DigestsFormatVersion, DIGESTS_FORMAT_VERSION,
};
pub use library::{
    MemoryAccessError, MemoryHit, MemoryItem, MemoryKind, MemoryLibrary, MemorySize,
};
pub use mention::{
    kraddie_chip_text, kraddie_mention_at, matching_kraddies, mentioned_kraddies, KraddieChip,
    KraddieMentionQuery, KRADDIE_MENTION_PREFIX,
};
pub use owner::{DigestId, MemoryOwner, MemoryRef, UnknownMemoryRef};
pub use pack::{
    chat_pack, estimate_tokens, mention_pack, rank, without_memory_packs, MemoryPack, PackRequest,
    CHARS_PER_TOKEN, CHAT_PACK_TOKENS, MEMORY_PACK_CLOSE, MEMORY_PACK_OPEN, MENTION_PACK_TOKENS,
};
pub use reach::{memory_reach, MemoryReach};
pub use timing::{
    digest_decision, ChatDigests, ChatPhase, DigestCheck, DigestDecision, DigestReason, DigestSkip,
    DIGEST_AFTER_QUIET, MIN_NEW_MESSAGES,
};
pub use writing::{
    digest_prompt, facts_from_reply, redacted, DIGEST_TRANSCRIPT_CHARS, NOTHING_TO_KEEP,
};
