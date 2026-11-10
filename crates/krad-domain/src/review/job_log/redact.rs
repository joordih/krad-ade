use std::borrow::Cow;
use std::ops::Range;

const REDACTED: &str = "[REDACTED]";
const TOKEN_PREFIXES: [&str; 25] = [
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "gloas-",
    "glrt-",
    "glptt-",
    "glcbt-",
    "sk-",
    "sk_live_",
    "sk_test_",
    "rk_live_",
    "AIza",
    "npm_",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xoxs-",
    "xapp-",
    "AKIA",
    "ASIA",
    "eyJ",
];
const MIN_TOKEN_LEN: usize = 16;
const MIN_OPAQUE_TOKEN_LEN: usize = 20;
const OPAQUE_TOKEN_PUNCTUATION: [char; 6] = ['_', '-', '.', '=', '+', '/'];
const PEM_BEGIN: &str = "-----BEGIN ";
const PEM_END: &str = "-----END ";
const PEM_PRIVATE_KEY: &str = "PRIVATE KEY-----";
const SECRET_KEYS: [&str; 12] = [
    "token",
    "secret",
    "password",
    "passwd",
    "pwd",
    "api_key",
    "apikey",
    "api-key",
    "access_key",
    "private_key",
    "credential",
    "authorization",
];
const AUTH_SCHEMES: [&str; 2] = ["bearer", "basic"];
const TOKEN_SCHEME: &str = "token";
const KEY_VALUE_SEPARATORS: [char; 2] = ['=', ':'];
const WORD_BREAKS: [char; 12] = ['"', '\'', '`', ',', ';', '(', ')', '{', '}', '<', '>', '&'];
const SCHEME_SEPARATOR: &str = "://";
const USERINFO_END: char = '@';
const USERINFO_SEPARATOR: char = ':';

#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    Anything,
    Separator,
    Value,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PemBlock {
    #[default]
    Outside,
    InsidePrivateKey,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SecretScanner {
    pem: PemBlock,
}

impl SecretScanner {
    pub(crate) fn redact<'a>(&mut self, line: &'a str) -> Cow<'a, str> {
        let names_key = line.contains(PEM_PRIVATE_KEY);
        match self.pem {
            PemBlock::InsidePrivateKey => {
                if line.contains(PEM_END) && names_key {
                    self.pem = PemBlock::Outside;
                }
                Cow::Borrowed(REDACTED)
            }
            PemBlock::Outside if line.contains(PEM_BEGIN) && names_key => {
                if !line.contains(PEM_END) {
                    self.pem = PemBlock::InsidePrivateKey;
                }
                Cow::Borrowed(REDACTED)
            }
            PemBlock::Outside => redact_secrets(line),
        }
    }
}

#[must_use]
pub(crate) fn redact_secrets(line: &str) -> Cow<'_, str> {
    let mut hidden: Vec<Range<usize>> = Vec::new();
    let mut expect = Expect::Anything;
    for word in words(line) {
        let text = &line[word.clone()];
        match expect {
            Expect::Value
                if is_auth_scheme(text)
                    || is_separator(text)
                    || text.eq_ignore_ascii_case(TOKEN_SCHEME) =>
            {
                continue
            }
            Expect::Value => {
                hidden.push(word);
                expect = Expect::Anything;
                continue;
            }
            Expect::Separator if is_separator(text) => {
                expect = Expect::Value;
                continue;
            }
            Expect::Separator if looks_opaque(text) => {
                hidden.push(word);
                expect = Expect::Anything;
                continue;
            }
            Expect::Separator | Expect::Anything => expect = Expect::Anything,
        }
        if let Some(range) = secret_value(text) {
            hidden.push(word.start + range.start..word.start + range.end);
        } else if is_auth_scheme(text) || names_secret_key(text) {
            expect = Expect::Value;
        } else if names_secret(text) {
            expect = Expect::Separator;
        } else if looks_like_token(text) {
            hidden.push(word);
        }
    }
    if hidden.is_empty() {
        return Cow::Borrowed(line);
    }
    let mut redacted = String::with_capacity(line.len());
    let mut from = 0;
    for range in hidden {
        redacted.push_str(&line[from..range.start]);
        redacted.push_str(REDACTED);
        from = range.end;
    }
    redacted.push_str(&line[from..]);
    Cow::Owned(redacted)
}

fn words(line: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut start = None;
    line.char_indices()
        .chain(std::iter::once((line.len(), ' ')))
        .filter_map(move |(at, character)| {
            let breaks = character.is_whitespace() || WORD_BREAKS.contains(&character);
            match (breaks, start) {
                (true, Some(from)) => {
                    start = None;
                    Some(from..at)
                }
                (false, None) => {
                    start = Some(at);
                    None
                }
                _ => None,
            }
        })
}

fn secret_value(word: &str) -> Option<Range<usize>> {
    if let Some(userinfo) = url_userinfo(word) {
        return Some(userinfo);
    }
    let at = word.find(KEY_VALUE_SEPARATORS)?;
    let value = at + 1..word.len();
    (names_secret(&word[..at]) && !value.is_empty() && !word[value.clone()].starts_with('/'))
        .then_some(value)
}

fn url_userinfo(word: &str) -> Option<Range<usize>> {
    let start = word.find(SCHEME_SEPARATOR)? + SCHEME_SEPARATOR.len();
    let end = start + word[start..].find(USERINFO_END)?;
    word[start..end]
        .contains(USERINFO_SEPARATOR)
        .then_some(start..end)
}

fn names_secret(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    !key.is_empty() && SECRET_KEYS.iter().any(|secret| key.contains(secret))
}

fn names_secret_key(word: &str) -> bool {
    word.ends_with(KEY_VALUE_SEPARATORS)
        && names_secret(word.trim_end_matches(KEY_VALUE_SEPARATORS))
}

fn is_auth_scheme(word: &str) -> bool {
    AUTH_SCHEMES
        .iter()
        .any(|scheme| word.eq_ignore_ascii_case(scheme))
}

fn is_separator(word: &str) -> bool {
    word.chars()
        .all(|character| KEY_VALUE_SEPARATORS.contains(&character))
}

fn looks_opaque(word: &str) -> bool {
    word.len() >= MIN_OPAQUE_TOKEN_LEN
        && word.chars().any(|character| character.is_ascii_digit())
        && word
            .chars()
            .any(|character| character.is_ascii_alphabetic())
        && word.chars().all(|character| {
            character.is_ascii_alphanumeric() || OPAQUE_TOKEN_PUNCTUATION.contains(&character)
        })
}

fn looks_like_token(word: &str) -> bool {
    word.len() >= MIN_TOKEN_LEN && TOKEN_PREFIXES.iter().any(|prefix| word.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRETS_FIXTURE: &str = include_str!("../../../fixtures/secrets_log.txt");
    const REDACTED_FIXTURE: &str = include_str!("../../../fixtures/secrets_log_redacted.txt");

    #[test]
    fn tokens_keys_and_url_logins_in_a_log_are_hidden() {
        let redacted: Vec<Cow<'_, str>> = SECRETS_FIXTURE.lines().map(redact_secrets).collect();
        let expected: Vec<&str> = REDACTED_FIXTURE.lines().collect();
        assert_eq!(redacted, expected);
    }

    #[test]
    fn a_private_key_block_is_hidden_line_by_line() {
        let mut scanner = SecretScanner::default();
        let lines = [
            "writing key",
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            "b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQ",
            "AAAAC3NzaC1lZDI1NTE5AAAAIMf",
            "-----END OPENSSH PRIVATE KEY-----",
            "-----BEGIN CERTIFICATE-----",
            "done",
        ];
        let redacted: Vec<Cow<'_, str>> = lines.iter().map(|line| scanner.redact(line)).collect();
        assert_eq!(
            redacted,
            [
                "writing key",
                REDACTED,
                REDACTED,
                REDACTED,
                REDACTED,
                "-----BEGIN CERTIFICATE-----",
                "done"
            ]
        );
        assert_eq!(scanner, SecretScanner::default());
    }

    #[test]
    fn ordinary_lines_are_left_alone() {
        let line = "test result: FAILED. 41 passed; 1 failed; token parsing ok";
        assert!(matches!(redact_secrets(line), Cow::Borrowed(_)));
        assert_eq!(redact_secrets("sk-short"), "sk-short");
        assert_eq!(
            redact_secrets("token handling took 1500ms in parser2"),
            "token handling took 1500ms in parser2"
        );
    }
}
