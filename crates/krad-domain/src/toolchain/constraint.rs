use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

use super::version::ToolchainVersion;

const ALTERNATIVE_SEPARATOR: &str = "||";
const CLAUSE_SEPARATOR: char = ',';
const HYPHEN_RANGE: &str = " - ";
const WILDCARDS: [&str; 3] = ["*", "x", "X"];
const PART_SEPARATOR: char = '.';
const FULL_VERSION_PARTS: usize = 3;
const VERSION_PREFIX: char = 'v';
const OPERATORS: [(&str, Operator); 11] = [
    ("===", Operator::Equal),
    ("~=", Operator::Compatible),
    ("==", Operator::Equal),
    ("!=", Operator::NotEqual),
    (">=", Operator::AtLeast),
    ("<=", Operator::AtMost),
    (">", Operator::Above),
    ("<", Operator::Below),
    ("^", Operator::Caret),
    ("~", Operator::Tilde),
    ("=", Operator::Equal),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operator {
    Equal,
    NotEqual,
    AtLeast,
    AtMost,
    Above,
    Below,
    Compatible,
    Caret,
    Tilde,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Comparator {
    operator: Operator,
    parts: Vec<u64>,
    open_tail: bool,
}

enum Clause {
    Any,
    Compare(Comparator),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct VersionConstraint {
    raw: String,
    alternatives: Vec<Vec<Comparator>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotAConstraint(pub String);

impl fmt::Display for NotAConstraint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "`{}` is not a version range", self.0)
    }
}

impl std::error::Error for NotAConstraint {}

fn compare(left: &[u64], right: &[u64]) -> Ordering {
    let len = left.len().max(right.len());
    (0..len)
        .map(|index| {
            let a = left.get(index).copied().unwrap_or_default();
            let b = right.get(index).copied().unwrap_or_default();
            a.cmp(&b)
        })
        .find(|ordering| ordering.is_ne())
        .unwrap_or(Ordering::Equal)
}

fn starts_with(version: &[u64], prefix: &[u64]) -> bool {
    prefix
        .iter()
        .enumerate()
        .all(|(index, part)| version.get(index).copied().unwrap_or_default() == *part)
}

fn bumped(parts: &[u64], index: usize) -> Vec<u64> {
    let mut next: Vec<u64> = parts.iter().take(index + 1).copied().collect();
    next.resize(index + 1, 0);
    next[index] = next[index].saturating_add(1);
    next
}

impl Comparator {
    fn matches(&self, version: &[u64]) -> bool {
        let wanted = self.parts.as_slice();
        match self.operator {
            Operator::Equal if self.open_tail => starts_with(version, wanted),
            Operator::Equal => compare(version, wanted).is_eq(),
            Operator::NotEqual if self.open_tail => !starts_with(version, wanted),
            Operator::NotEqual => compare(version, wanted).is_ne(),
            Operator::AtLeast => compare(version, wanted).is_ge(),
            Operator::AtMost if self.open_tail => {
                compare(version, &bumped(wanted, wanted.len().saturating_sub(1))).is_lt()
            }
            Operator::AtMost => compare(version, wanted).is_le(),
            Operator::Above if self.open_tail => {
                compare(version, &bumped(wanted, wanted.len().saturating_sub(1))).is_ge()
            }
            Operator::Above => compare(version, wanted).is_gt(),
            Operator::Below => compare(version, wanted).is_lt(),
            Operator::Compatible => {
                let kept = wanted.len().saturating_sub(1).max(1);
                compare(version, wanted).is_ge() && starts_with(version, &wanted[..kept])
            }
            Operator::Caret => {
                let index = wanted
                    .iter()
                    .position(|part| *part != 0)
                    .unwrap_or(wanted.len().saturating_sub(1));
                compare(version, wanted).is_ge() && compare(version, &bumped(wanted, index)).is_lt()
            }
            Operator::Tilde => {
                let index = usize::from(wanted.len() > 1);
                compare(version, wanted).is_ge() && compare(version, &bumped(wanted, index)).is_lt()
            }
        }
    }
}

fn version_parts(text: &str) -> Option<(Vec<u64>, bool)> {
    let text = text.trim().trim_start_matches(VERSION_PREFIX);
    let mut parts = Vec::new();
    let mut open_tail = false;
    for piece in text.split(PART_SEPARATOR) {
        if WILDCARDS.contains(&piece) {
            open_tail = true;
            break;
        }
        let digits: String = piece.chars().take_while(char::is_ascii_digit).collect();
        let number = digits.parse::<u64>().ok()?;
        parts.push(number);
        if digits.len() != piece.len() {
            break;
        }
    }
    if parts.is_empty() && !open_tail {
        return None;
    }
    Some((parts, open_tail))
}

fn comparator(text: &str) -> Option<Clause> {
    let text = text.trim();
    if text.is_empty() || WILDCARDS.contains(&text) {
        return Some(Clause::Any);
    }
    let (operator, rest) = OPERATORS
        .iter()
        .find_map(|(symbol, operator)| text.strip_prefix(symbol).map(|rest| (*operator, rest)))
        .unwrap_or((Operator::Equal, text));
    let (parts, open_tail) = version_parts(rest)?;
    if parts.is_empty() {
        return Some(Clause::Any);
    }
    let implicit_tail = operator == Operator::Equal && !text.starts_with('=');
    Some(Clause::Compare(Comparator {
        operator,
        open_tail: open_tail || implicit_tail,
        parts,
    }))
}

fn joined_operators(text: &str) -> String {
    let mut joined = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        joined.push(character);
        if matches!(character, '<' | '>' | '=' | '~' | '^' | '!') {
            while chars.peek().is_some_and(|next| *next == ' ') {
                chars.next();
            }
        }
    }
    joined
}

fn clauses(text: &str) -> Option<Vec<Comparator>> {
    if let Some((low, high)) = text.split_once(HYPHEN_RANGE) {
        let (low, _) = version_parts(low)?;
        let (high, high_open) = version_parts(high)?;
        let high_partial = high_open || high.len() < FULL_VERSION_PARTS;
        return Some(vec![
            Comparator {
                operator: Operator::AtLeast,
                parts: low,
                open_tail: false,
            },
            Comparator {
                operator: Operator::AtMost,
                parts: high,
                open_tail: high_partial,
            },
        ]);
    }
    let normalized = joined_operators(text);
    let mut found = Vec::new();
    for piece in normalized
        .split(CLAUSE_SEPARATOR)
        .flat_map(str::split_whitespace)
    {
        if let Clause::Compare(clause) = comparator(piece)? {
            found.push(clause);
        }
    }
    Some(found)
}

impl VersionConstraint {
    #[must_use]
    pub fn at_least(parts: &[u64]) -> Self {
        let shown: Vec<String> = parts.iter().map(u64::to_string).collect();
        Self {
            raw: format!(">={}", shown.join(".")),
            alternatives: vec![vec![Comparator {
                operator: Operator::AtLeast,
                parts: parts.to_vec(),
                open_tail: false,
            }]],
        }
    }

    #[must_use]
    pub fn matches(&self, version: &ToolchainVersion) -> bool {
        self.alternatives
            .iter()
            .any(|clauses| clauses.iter().all(|clause| clause.matches(version.parts())))
    }

    #[must_use]
    pub fn and(&self, other: &Self) -> Self {
        let mut alternatives = Vec::new();
        for left in &self.alternatives {
            for right in &other.alternatives {
                alternatives.push(left.iter().chain(right).cloned().collect());
            }
        }
        Self {
            raw: format!("{}, {}", self.raw, other.raw),
            alternatives,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl std::str::FromStr for VersionConstraint {
    type Err = NotAConstraint;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let raw = text.trim();
        let alternatives = raw
            .split(ALTERNATIVE_SEPARATOR)
            .map(|alternative| clauses(alternative.trim()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| NotAConstraint(raw.to_owned()))?;
        Ok(Self {
            raw: raw.to_owned(),
            alternatives,
        })
    }
}

impl TryFrom<String> for VersionConstraint {
    type Error = NotAConstraint;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        raw.parse()
    }
}

impl From<VersionConstraint> for String {
    fn from(constraint: VersionConstraint) -> Self {
        constraint.raw
    }
}

impl fmt::Display for VersionConstraint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn holds(constraint: &str, version: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let constraint: VersionConstraint = constraint.parse()?;
        Ok(constraint.matches(&version.parse()?))
    }

    #[test]
    fn python_requirements_follow_pep_440() -> TestResult {
        assert!(!holds(">=3.10", "3.9.6")?);
        assert!(holds(">=3.10", "3.10.0")?);
        assert!(holds(">=3.10", "3.13.1")?);
        assert!(holds(">=3.8, <4", "3.12.4")?);
        assert!(!holds(">=3.8, <4", "4.0")?);
        assert!(holds("~=3.9", "3.12")?);
        assert!(!holds("~=3.9.2", "3.10.0")?);
        assert!(!holds("!=3.0.*", "3.0.5")?);
        assert!(holds("!=3.0.*", "3.1")?);
        assert!(holds("==3.*", "3.11.2")?);
        Ok(())
    }

    #[test]
    fn node_engines_follow_npm_ranges() -> TestResult {
        assert!(holds(">=18.17.0", "20.11.1")?);
        assert!(!holds(">=18.17.0", "18.16.0")?);
        assert!(holds("^18 || >=20", "21.1.0")?);
        assert!(!holds("^18 || >=20", "19.0.0")?);
        assert!(holds("^0.2.3", "0.2.9")?);
        assert!(!holds("^0.2.3", "0.3.0")?);
        assert!(holds("~1.2", "1.2.9")?);
        assert!(!holds("~1.2", "1.3.0")?);
        assert!(holds("18.x", "18.4.0")?);
        assert!(holds("16 - 18", "18.9.0")?);
        assert!(!holds("16 - 18", "19.0.0")?);
        assert!(holds(">= 14", "22.0.0")?);
        assert!(holds("*", "1.0")?);
        Ok(())
    }

    #[test]
    fn ranges_combine_and_reject_garbage() -> TestResult {
        let manifest = VersionConstraint::at_least(&[3]);
        let package: VersionConstraint = ">=3.10".parse()?;
        let both = manifest.and(&package);
        assert!(!both.matches(&"3.9.6".parse()?));
        assert!(both.matches(&"3.10.1".parse()?));
        assert!("not a range".parse::<VersionConstraint>().is_err());
        Ok(())
    }
}
