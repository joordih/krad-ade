use super::library::MemorySize;
use super::pack::MemoryPack;
use crate::kraddie::{KraddieId, KraddieName};
use std::ops::Range;

pub const KRADDIE_MENTION_PREFIX: char = '$';
const NOTHING_KEPT: &str = "Nothing kept yet";
const TOOLTIP_SEPARATOR: &str = "\n";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KraddieMentionQuery {
    pub range: Range<usize>,
    pub query: String,
}

#[must_use]
pub fn kraddie_mention_at(text: &str, cursor: usize) -> Option<KraddieMentionQuery> {
    let before = text.get(..cursor)?;
    let start = before
        .char_indices()
        .rev()
        .find(|(_, char)| char.is_whitespace())
        .map_or(0, |(at, char)| at + char.len_utf8());
    let query = before[start..].strip_prefix(KRADDIE_MENTION_PREFIX)?;
    if query.starts_with(|char: char| char.is_ascii_digit()) {
        return None;
    }
    Some(KraddieMentionQuery {
        range: start..cursor,
        query: query.to_owned(),
    })
}

fn fuzzy_score(candidate: &str, query: &str) -> Option<u32> {
    let candidate = candidate.to_lowercase();
    let query = query.to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    if candidate.starts_with(&query) {
        return Some(u32::MAX);
    }
    let mut chars = candidate.chars();
    let mut gaps: u32 = 0;
    for wanted in query.chars() {
        let mut skipped = 0;
        loop {
            let found = chars.next()?;
            if found == wanted {
                break;
            }
            skipped += 1;
        }
        gaps = gaps.saturating_add(skipped);
    }
    Some((u32::MAX / 2).saturating_sub(gaps))
}

#[must_use]
pub fn matching_kraddies<'a>(
    names: impl IntoIterator<Item = &'a KraddieName>,
    query: &str,
) -> Vec<&'a KraddieName> {
    let mut scored: Vec<(u32, &KraddieName)> = names
        .into_iter()
        .filter_map(|name| {
            let by_name = fuzzy_score(name.as_str(), query);
            let by_id = fuzzy_score(name.id().as_str(), query);
            by_name.max(by_id).map(|score| (score, name))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    scored.into_iter().map(|(_, name)| name).collect()
}

#[must_use]
pub fn kraddie_chip_text(id: &KraddieId) -> String {
    format!("{KRADDIE_MENTION_PREFIX}{id}")
}

#[must_use]
pub fn mentioned_kraddies(text: &str, known: &[KraddieId]) -> Vec<KraddieId> {
    let mut found: Vec<KraddieId> = Vec::new();
    for word in text.split(|char: char| char.is_whitespace()) {
        let Some(slug) = word.strip_prefix(KRADDIE_MENTION_PREFIX) else {
            continue;
        };
        let slug = slug.trim_end_matches(|char: char| !char.is_alphanumeric());
        let Some(id) = known.iter().find(|id| id.as_str() == slug) else {
            continue;
        };
        if !found.contains(id) {
            found.push(id.clone());
        }
    }
    found
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KraddieChip {
    pub kraddie: KraddieId,
    pub name: KraddieName,
    pub size: MemorySize,
    pub pack: MemoryPack,
}

impl KraddieChip {
    #[must_use]
    pub fn text(&self) -> String {
        kraddie_chip_text(&self.kraddie)
    }

    #[must_use]
    pub fn tooltip(&self) -> String {
        if self.size.is_empty() {
            return format!("{}{TOOLTIP_SEPARATOR}{NOTHING_KEPT}", self.name);
        }
        format!(
            "{}{TOOLTIP_SEPARATOR}{}{TOOLTIP_SEPARATOR}{}",
            self.name, self.size, self.pack
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> KraddieName {
        text.parse().unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn a_dollar_word_opens_the_kraddie_popup() {
        assert_eq!(
            kraddie_mention_at("ask $nig", 8),
            Some(KraddieMentionQuery {
                range: 4..8,
                query: "nig".to_owned()
            })
        );
        assert_eq!(
            kraddie_mention_at("$", 1).map(|found| found.query),
            Some(String::new())
        );
        assert_eq!(kraddie_mention_at("costs $5", 8), None);
        assert_eq!(kraddie_mention_at("a$b", 3), None);
        assert_eq!(kraddie_mention_at("$owl done", 9), None);
    }

    #[test]
    fn the_filter_is_fuzzy_and_prefers_prefixes() {
        let names = [name("Night Owl"), name("Linter"), name("Owlet")];
        let found: Vec<&str> = matching_kraddies(&names, "owl")
            .into_iter()
            .map(KraddieName::as_str)
            .collect();
        assert_eq!(found, ["Owlet", "Night Owl"]);
        assert_eq!(matching_kraddies(&names, "ntr").len(), 1);
        assert_eq!(matching_kraddies(&names, "").len(), 3);
        assert_eq!(matching_kraddies(&names, "zzz").len(), 0);
    }

    #[test]
    fn mentions_are_read_back_from_the_sent_text() {
        let known = [
            KraddieId::try_from("night-owl".to_owned()).unwrap_or_else(|_| unreachable!()),
            KraddieId::try_from("linter".to_owned()).unwrap_or_else(|_| unreachable!()),
        ];
        assert_eq!(
            mentioned_kraddies("ask $night-owl, then $linter and $night-owl $ghost", &known),
            known.to_vec()
        );
        assert_eq!(mentioned_kraddies("costs $5", &known).len(), 0);
    }

    #[test]
    fn the_chip_names_its_memory_and_any_truncation() {
        let chip = KraddieChip {
            kraddie: name("Night Owl").id(),
            name: name("Night Owl"),
            size: MemorySize {
                keepsakes: 3,
                digests: 2,
            },
            pack: MemoryPack {
                included: 4,
                left_out: 1,
                tokens: 700,
                budget: 800,
                ..MemoryPack::default()
            },
        };
        assert_eq!(chip.text(), "$night-owl");
        assert_eq!(
            chip.tooltip(),
            "Night Owl\n3 keepsakes · 2 chat digests\n4 memories attached, about 700 of 800 tokens; 1 left out to stay in budget"
        );
        let empty = KraddieChip {
            size: MemorySize::default(),
            ..chip
        };
        assert_eq!(empty.tooltip(), "Night Owl\nNothing kept yet");
    }
}
