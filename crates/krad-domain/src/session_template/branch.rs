use std::fmt;

const SEPARATOR: char = '-';
const MAX_BRANCH_LEN: usize = 60;
const FOLDED: [(&str, &str); 15] = [
    ("àáâãäåā", "a"),
    ("æ", "ae"),
    ("çćč", "c"),
    ("ď", "d"),
    ("èéêëēěę", "e"),
    ("ìíîïī", "i"),
    ("ł", "l"),
    ("ñńň", "n"),
    ("òóôõöøō", "o"),
    ("œ", "oe"),
    ("ŕř", "r"),
    ("śšß", "s"),
    ("ùúûüūů", "u"),
    ("ýÿ", "y"),
    ("žźż", "z"),
];

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BranchName(String);

impl BranchName {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BranchName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn fold(character: char) -> Option<&'static str> {
    FOLDED
        .iter()
        .find(|(letters, _)| letters.contains(character))
        .map(|(_, ascii)| *ascii)
}

pub(super) fn words_slug(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    for character in text.chars().flat_map(char::to_lowercase) {
        if let Some(ascii) = fold(character) {
            slug.push_str(ascii);
        } else if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.is_empty() && !slug.ends_with(SEPARATOR) {
            slug.push(SEPARATOR);
        }
    }
    slug.trim_end_matches(SEPARATOR).to_owned()
}

#[must_use]
pub fn branch_slug(text: &str) -> Option<BranchName> {
    let slug = words_slug(text);
    let cut = slug
        .char_indices()
        .nth(MAX_BRANCH_LEN)
        .map_or(slug.as_str(), |(end, _)| &slug[..end]);
    let cut = if cut.len() < slug.len() && !slug[cut.len()..].starts_with(SEPARATOR) {
        cut.rfind(SEPARATOR).map_or(cut, |end| &cut[..end])
    } else {
        cut
    };
    let cut = cut.trim_matches(SEPARATOR);
    (!cut.is_empty()).then(|| BranchName(cut.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slug(text: &str) -> Option<String> {
        branch_slug(text).map(|branch| branch.as_str().to_owned())
    }

    #[test]
    fn a_branch_slug_is_lowercase_ascii_words_joined_by_dashes() {
        assert_eq!(
            slug("Fix: refund fails on Safari!").as_deref(),
            Some("fix-refund-fails-on-safari")
        );
        assert_eq!(
            slug("  #42 -- Login/Logout  ").as_deref(),
            Some("42-login-logout")
        );
        assert_eq!(
            slug("Añadir canción a la lista").as_deref(),
            Some("anadir-cancion-a-la-lista")
        );
        assert_eq!(slug("?!  ..").as_deref(), None);
        assert_eq!(slug("日本語").as_deref(), None);
    }

    #[test]
    fn a_long_branch_slug_is_cut_at_a_word() {
        let title = "make the checkout page remember the selected shipping address between visits";
        let cut = slug(title).unwrap_or_default();
        assert!(cut.len() <= MAX_BRANCH_LEN);
        assert_eq!(cut, "make-the-checkout-page-remember-the-selected-shipping");
    }
}
