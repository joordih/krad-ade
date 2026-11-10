#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContextSize {
    OneMillion,
}

impl ContextSize {
    const SUFFIXES: [(&'static str, Self); 1] = [("[1m]", Self::OneMillion)];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::OneMillion => "1M context",
        }
    }
}

const CLAUDE_PREFIX: &str = "claude";
pub const CLAUDE_BRAND: &str = "Claude";
const GPT_PREFIX: &str = "gpt";
const GPT_BRAND: &str = "GPT";
const PROVIDER_SEPARATOR: char = '/';
const WORD_SEPARATORS: [char; 3] = ['-', '_', ' '];
const VERSION_SEPARATOR: &str = ".";
const DATE_DIGITS: usize = 8;
const REASONING_MODEL_LETTER: char = 'o';

const CLAUDE_ALIASES: [(&str, &str); 7] = [
    ("default", "Default"),
    ("opus", "Claude Opus"),
    ("sonnet", "Claude Sonnet"),
    ("haiku", "Claude Haiku"),
    ("fable", "Claude Fable"),
    ("opusplan", "Claude Opus in plan mode, Sonnet otherwise"),
    ("best", "The most capable Claude model"),
];

const BRANDS: [(&str, &str); 9] = [
    ("glm", "GLM"),
    ("oss", "OSS"),
    ("deepseek", "DeepSeek"),
    ("xai", "xAI"),
    ("qwen", "Qwen"),
    ("llama", "Llama"),
    ("mistral", "Mistral"),
    ("gemini", "Gemini"),
    ("grok", "Grok"),
];

#[must_use]
pub fn split_context(id: &str) -> (&str, Option<ContextSize>) {
    ContextSize::SUFFIXES
        .iter()
        .find_map(|(suffix, size)| {
            id.to_ascii_lowercase()
                .ends_with(suffix)
                .then(|| (&id[..id.len() - suffix.len()], Some(*size)))
        })
        .unwrap_or((id, None))
}

#[must_use]
pub fn claude_alias_name(alias: &str) -> Option<&'static str> {
    let (alias, _) = split_context(alias);
    CLAUDE_ALIASES
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(alias))
        .map(|(_, name)| *name)
}

fn is_number(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_digit())
}

fn is_date(word: &str) -> bool {
    word.len() == DATE_DIGITS && is_number(word)
}

fn title_word(word: &str) -> String {
    let lower = word.to_ascii_lowercase();
    if let Some((_, brand)) = BRANDS.iter().find(|(known, _)| *known == lower) {
        return (*brand).to_owned();
    }
    let mut chars = lower.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let rest: String = chars.collect();
    if first == REASONING_MODEL_LETTER && is_number(&rest) {
        return lower;
    }
    if first.is_ascii_digit() || rest.chars().any(|c| c.is_ascii_digit()) {
        return lower.to_ascii_uppercase();
    }
    first.to_ascii_uppercase().to_string() + &rest
}

fn joined_words(words: &[&str]) -> Vec<String> {
    let mut joined: Vec<String> = Vec::with_capacity(words.len());
    let mut previous_number = false;
    for word in words {
        let number = is_number(word);
        match joined.last_mut() {
            Some(last) if number && previous_number => {
                last.push_str(VERSION_SEPARATOR);
                last.push_str(word);
            }
            _ => joined.push(title_word(word)),
        }
        previous_number = number;
    }
    joined
}

fn claude_name(words: &[&str]) -> String {
    let (numbers, family): (Vec<&str>, Vec<&str>) = words.iter().partition(|word| is_number(word));
    let mut parts = vec![CLAUDE_BRAND.to_owned()];
    parts.extend(family.iter().map(|word| title_word(word)));
    if !numbers.is_empty() {
        parts.push(numbers.join(VERSION_SEPARATOR));
    }
    parts.join(" ")
}

fn gpt_name(words: &[&str]) -> String {
    let mut rest = joined_words(words);
    match rest.first_mut() {
        Some(version) if version.starts_with(|c: char| c.is_ascii_digit()) => {
            *version = format!("{GPT_BRAND}-{version}");
        }
        _ => rest.insert(0, GPT_BRAND.to_owned()),
    }
    rest.join(" ")
}

#[must_use]
pub fn humanize_model_id(id: &str) -> Option<String> {
    let (base, _) = split_context(id.trim());
    let base = base.rsplit(PROVIDER_SEPARATOR).next().unwrap_or(base);
    let mut words: Vec<&str> = base
        .split(WORD_SEPARATORS)
        .filter(|word| !word.is_empty())
        .collect();
    if words.last().is_some_and(|word| is_date(word)) {
        words.pop();
    }
    let (first, rest) = words.split_first()?;
    let first = first.to_ascii_lowercase();
    if first == CLAUDE_PREFIX {
        return Some(claude_name(rest));
    }
    if first == GPT_PREFIX {
        return Some(gpt_name(rest));
    }
    if let Some(version) = first.strip_prefix(GPT_PREFIX) {
        let mut tail = vec![version];
        tail.extend_from_slice(rest);
        return Some(gpt_name(&tail));
    }
    Some(joined_words(&words).join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(id: &str) -> String {
        humanize_model_id(id).unwrap_or_default()
    }

    #[test]
    fn claude_ids_read_as_family_and_version() {
        assert_eq!(named("claude-opus-5-5"), "Claude Opus 5.5");
        assert_eq!(named("claude-fable-5-2"), "Claude Fable 5.2");
        assert_eq!(named("claude-haiku-5-5"), "Claude Haiku 5.5");
        assert_eq!(named("claude-sonnet-4-5-20250929"), "Claude Sonnet 4.5");
        assert_eq!(named("claude-3-5-sonnet-20241022"), "Claude Sonnet 3.5");
        assert_eq!(named("anthropic/claude-sonnet-5-5"), "Claude Sonnet 5.5");
        assert_eq!(named("claude-fable-5-1[1m]"), "Claude Fable 5.1");
    }

    #[test]
    fn other_vendors_keep_their_brand_spelling() {
        assert_eq!(named("gpt-6.1-sol"), "GPT-6.1 Sol");
        assert_eq!(named("gpt-5.1-codex-max"), "GPT-5.1 Codex Max");
        assert_eq!(named("gpt-oss-120b"), "GPT OSS 120B");
        assert_eq!(named("o3"), "o3");
        assert_eq!(named("gemini-2-5-pro"), "Gemini 2.5 Pro");
        assert_eq!(named("opencode-go/glm-5.3-flash"), "GLM 5.3 Flash");
        assert_eq!(named("kimi-k2.7-code"), "Kimi K2.7 Code");
        assert_eq!(humanize_model_id(""), None);
        assert_eq!(humanize_model_id("--"), None);
    }

    #[test]
    fn the_one_million_suffix_is_a_context_size() {
        assert_eq!(
            split_context("claude-fable-5-1[1m]"),
            ("claude-fable-5-1", Some(ContextSize::OneMillion))
        );
        assert_eq!(
            split_context("sonnet[1M]"),
            ("sonnet", Some(ContextSize::OneMillion))
        );
        assert_eq!(split_context("opus"), ("opus", None));
    }

    #[test]
    fn claude_aliases_fall_back_to_a_built_in_name() {
        assert_eq!(claude_alias_name("opus"), Some("Claude Opus"));
        assert_eq!(claude_alias_name("Sonnet[1m]"), Some("Claude Sonnet"));
        assert_eq!(claude_alias_name("default"), Some("Default"));
        assert_eq!(claude_alias_name("claude-opus-5-5"), None);
    }
}
