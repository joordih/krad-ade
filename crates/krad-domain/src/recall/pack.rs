use super::library::{query_terms, relevance, MemoryItem, MemoryKind};
use super::owner::MemoryOwner;
use std::borrow::Cow;
use std::cmp::Reverse;
use std::fmt;
use std::time::{Duration, SystemTime};

pub const CHARS_PER_TOKEN: usize = 4;
pub const CHAT_PACK_TOKENS: usize = 1_500;
pub const MENTION_PACK_TOKENS: usize = 800;
const RELEVANCE_WEIGHT: u64 = 1_000;
const HOME_BONUS: u64 = 500;
const KEEPSAKE_BONUS: u64 = 300;
const RECENCY_STEP: Duration = Duration::from_hours(24);
const RECENCY_DAYS: u64 = 90;
const RECENCY_WEIGHT: u64 = 2;
const PACK_HEADING: &str = "Memory from earlier chats. Ask for more with the memory_search and memory_read tools of the krad-code server:";
const MENTION_HEADING: &str = "Memory of the Kraddie";
const MENTION_HEADING_TAIL: &str = ", attached by the user:";
const LINE: char = '\n';
const BULLET: &str = "- ";
const TAG_OPEN: char = '[';
const TAG_CLOSE: &str = "] ";
const TAG_SEPARATOR: &str = " · ";
const KEEPSAKE_TAG: &str = "keepsake";
pub const MEMORY_PACK_OPEN: &str = "<krad-memory>";
pub const MEMORY_PACK_CLOSE: &str = "</krad-memory>";

#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(CHARS_PER_TOKEN)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryPack {
    pub text: String,
    pub included: usize,
    pub left_out: usize,
    pub tokens: usize,
    pub budget: usize,
}

impl MemoryPack {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.included == 0
    }

    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.left_out > 0
    }
}

impl fmt::Display for MemoryPack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} memories attached, about {} of {} tokens",
            self.included, self.tokens, self.budget
        )?;
        if self.truncated() {
            write!(f, "; {} left out to stay in budget", self.left_out)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PackRequest<'a> {
    pub home: &'a MemoryOwner,
    pub query: &'a str,
    pub budget: usize,
    pub now: SystemTime,
}

fn recency(item: &MemoryItem, now: SystemTime) -> u64 {
    let Some(written) = item.written else {
        return RECENCY_DAYS * RECENCY_WEIGHT;
    };
    let age = now.duration_since(written).unwrap_or_default();
    let days = (age.as_secs() / RECENCY_STEP.as_secs()).min(RECENCY_DAYS);
    (RECENCY_DAYS - days) * RECENCY_WEIGHT
}

fn line_of(item: &MemoryItem) -> String {
    let mut line = String::from(BULLET);
    line.push(TAG_OPEN);
    line.push_str(&item.owner().to_string());
    line.push_str(TAG_SEPARATOR);
    match (&item.kind, &item.title) {
        (MemoryKind::Digest, Some(title)) => line.push_str(title),
        _ => line.push_str(KEEPSAKE_TAG),
    }
    line.push_str(TAG_SEPARATOR);
    line.push_str(&item.reference.to_string());
    line.push_str(TAG_CLOSE);
    line.push_str(&item.text);
    line
}

#[must_use]
pub fn rank<'a>(
    items: impl IntoIterator<Item = &'a MemoryItem>,
    request: &PackRequest<'_>,
) -> Vec<&'a MemoryItem> {
    let terms = query_terms(request.query);
    let mut scored: Vec<(u64, &MemoryItem)> = items
        .into_iter()
        .map(|item| {
            let mut score = u64::from(relevance(item, &terms)) * RELEVANCE_WEIGHT;
            score += recency(item, request.now);
            if item.owner() == *request.home {
                score += HOME_BONUS;
            }
            if item.kind == MemoryKind::Keepsake {
                score += KEEPSAKE_BONUS;
            }
            (score, item)
        })
        .collect();
    scored.sort_by_key(|(score, item)| (Reverse(*score), item.reference.clone()));
    scored.into_iter().map(|(_, item)| item).collect()
}

fn pack_with<'a>(
    heading: &str,
    items: impl IntoIterator<Item = &'a MemoryItem>,
    request: &PackRequest<'_>,
) -> MemoryPack {
    let ranked = rank(items, request);
    let mut text = format!("{MEMORY_PACK_OPEN}{LINE}{heading}");
    let mut tokens = estimate_tokens(&text) + estimate_tokens(MEMORY_PACK_CLOSE) + 1;
    let mut included = 0;
    let mut left_out = 0;
    for item in ranked {
        let line = line_of(item);
        let cost = estimate_tokens(&line) + 1;
        if tokens + cost > request.budget {
            left_out += 1;
            continue;
        }
        text.push(LINE);
        text.push_str(&line);
        tokens += cost;
        included += 1;
    }
    if included == 0 {
        text.clear();
        tokens = 0;
    } else {
        text.push(LINE);
        text.push_str(MEMORY_PACK_CLOSE);
    }
    MemoryPack {
        text,
        included,
        left_out,
        tokens,
        budget: request.budget,
    }
}

#[must_use]
pub fn chat_pack<'a>(
    items: impl IntoIterator<Item = &'a MemoryItem>,
    request: &PackRequest<'_>,
) -> MemoryPack {
    pack_with(PACK_HEADING, items, request)
}

#[must_use]
pub fn mention_pack<'a>(
    name: &str,
    items: impl IntoIterator<Item = &'a MemoryItem>,
    request: &PackRequest<'_>,
) -> MemoryPack {
    let heading = format!("{MENTION_HEADING} {name}{MENTION_HEADING_TAIL}");
    pack_with(&heading, items, request)
}

#[must_use]
pub fn without_memory_packs(text: &str) -> Cow<'_, str> {
    if !text.contains(MEMORY_PACK_OPEN) {
        return Cow::Borrowed(text);
    }
    let mut kept = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(MEMORY_PACK_OPEN) {
        kept.push_str(&rest[..start]);
        let block = &rest[start..];
        rest = block
            .find(MEMORY_PACK_CLOSE)
            .map_or("", |end| &block[end + MEMORY_PACK_CLOSE.len()..]);
    }
    kept.push_str(rest);
    Cow::Owned(kept.trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::super::library::tests::{id, library};
    use super::super::reach::memory_reach;
    use super::*;

    fn request<'a>(home: &'a MemoryOwner, query: &'a str, budget: usize) -> PackRequest<'a> {
        PackRequest {
            home,
            query,
            budget,
            now: SystemTime::UNIX_EPOCH + Duration::from_hours(24 * 30),
        }
    }

    #[test]
    fn tokens_are_estimated_from_characters() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
    }

    #[test]
    fn the_pack_ranks_relevant_and_own_memory_first() {
        let (library, kraddies) = library();
        let home = MemoryOwner::Kraddie(id("a"));
        let reach = memory_reach(&home, &kraddies, &[]);
        let pack = chat_pack(library.visible(&reach), &request(&home, "postgres", 2_000));
        assert_eq!(pack.included, 4);
        assert!(!pack.truncated());
        let lines: Vec<&str> = pack.text.lines().skip(2).collect();
        assert!(lines[0].contains("Postgres"), "{lines:?}");
        assert!(lines[1].contains("Postgres"), "{lines:?}");
        assert!(lines[2].contains("keepsake"), "{lines:?}");
        assert!(pack.text.starts_with(MEMORY_PACK_OPEN));
        assert!(pack
            .text
            .lines()
            .nth(1)
            .is_some_and(|line| line == PACK_HEADING));
        assert!(pack.text.ends_with(MEMORY_PACK_CLOSE));
        assert!(pack.tokens <= pack.budget);
    }

    #[test]
    fn a_tight_budget_leaves_items_out_and_says_so() {
        let (library, kraddies) = library();
        let home = MemoryOwner::Kraddie(id("a"));
        let reach = memory_reach(&home, &kraddies, &[]);
        let budget = estimate_tokens(PACK_HEADING) + 50;
        let pack = chat_pack(library.visible(&reach), &request(&home, "", budget));
        assert!(pack.included >= 1);
        assert!(pack.truncated());
        assert_eq!(pack.included + pack.left_out, 4);
        assert!(pack.tokens <= budget);
        assert!(pack.to_string().contains("left out to stay in budget"));
        let none = chat_pack(library.visible(&reach), &request(&home, "", 1));
        assert!(none.is_empty());
        assert_eq!(none.text.len(), 0);
    }

    #[test]
    fn a_mention_pack_holds_only_the_named_kraddie() {
        let (library, _) = library();
        let owner = MemoryOwner::Kraddie(id("b"));
        let pack = mention_pack(
            "B",
            library.of_owner(&owner),
            &request(&MemoryOwner::Ordinary, "", MENTION_PACK_TOKENS),
        );
        assert_eq!(pack.included, 1);
        assert!(pack
            .text
            .lines()
            .nth(1)
            .is_some_and(|line| line.starts_with("Memory of the Kraddie B, attached")));
        assert!(pack.text.contains("Postgres 17"));
    }

    #[test]
    fn packs_are_cut_out_of_a_message_wherever_they_sit() {
        let (library, _) = library();
        let owner = MemoryOwner::Kraddie(id("b"));
        let pack = mention_pack(
            "B",
            library.of_owner(&owner),
            &request(&MemoryOwner::Ordinary, "", MENTION_PACK_TOKENS),
        );
        let message = format!("Fix the deploy $b\n\n{}\n\n{}", pack.text, pack.text);
        assert_eq!(without_memory_packs(&message), "Fix the deploy $b");
        assert_eq!(without_memory_packs("plain words"), "plain words");
        let torn = format!("before {MEMORY_PACK_OPEN} never closed");
        assert_eq!(without_memory_packs(&torn), "before");
    }
}
