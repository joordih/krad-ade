use super::tokens::tokens;

const MIN_QUERY_CHARS: usize = 2;
pub(super) const PHRASE_SEPARATOR: &str = " ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryQuery {
    pub terms: Vec<String>,
    pub open_ended: bool,
}

impl HistoryQuery {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.trim().chars().count() < MIN_QUERY_CHARS {
            return None;
        }
        let mut terms: Vec<String> = Vec::new();
        for token in tokens(text) {
            if !terms.contains(&token.term) {
                terms.push(token.term);
            }
        }
        let open_ended = text.chars().last().is_some_and(char::is_alphanumeric);
        (!terms.is_empty()).then_some(Self { terms, open_ended })
    }

    pub(super) fn is_prefix(&self, index: usize) -> bool {
        self.open_ended && index + 1 == self.terms.len()
    }

    pub(super) fn matches(&self, index: usize, term: &str) -> bool {
        let Some(wanted) = self.terms.get(index) else {
            return false;
        };
        term == wanted || (self.is_prefix(index) && term.starts_with(wanted.as_str()))
    }

    pub(super) fn phrase(&self) -> Option<String> {
        (self.terms.len() > 1).then(|| self.terms.join(PHRASE_SEPARATOR))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(query: &str, index: usize, term: &str) -> bool {
        HistoryQuery::parse(query).is_some_and(|query| query.matches(index, term))
    }

    #[test]
    fn a_query_keeps_its_distinct_terms_in_order() {
        let terms = HistoryQuery::parse("Race in the race tracker").map(|query| query.terms);
        assert_eq!(
            terms,
            Some(["race", "in", "the", "tracker"].map(str::to_owned).to_vec())
        );
    }

    #[test]
    fn the_last_term_is_a_prefix_only_while_it_is_still_being_typed() {
        assert!(matches("tail rea", 1, "reader"));
        assert!(!matches("tail rea", 0, "tails"));
        assert!(!matches("tail rea ", 1, "reader"));
        assert!(matches("tail rea ", 1, "rea"));
    }

    #[test]
    fn too_short_or_wordless_queries_find_nothing() {
        assert_eq!(HistoryQuery::parse(" a "), None);
        assert_eq!(HistoryQuery::parse("--"), None);
        assert!(HistoryQuery::parse("ab").is_some());
    }

    #[test]
    fn only_several_terms_make_a_phrase() {
        let phrase = |text| HistoryQuery::parse(text).and_then(|query| query.phrase());
        assert_eq!(phrase("one"), None);
        assert_eq!(phrase("Tail, reader").as_deref(), Some("tail reader"));
    }
}
