use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub term: String,
    pub span: Range<usize>,
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric()
}

pub fn tokens(text: &str) -> impl Iterator<Item = Token> + '_ {
    let mut chars = text.char_indices().peekable();
    std::iter::from_fn(move || {
        let (start, _) = chars.by_ref().find(|(_, character)| is_word(*character))?;
        let mut end = text.len();
        while let Some((index, character)) = chars.peek().copied() {
            if !is_word(character) {
                end = index;
                break;
            }
            chars.next();
        }
        let word = &text[start..end];
        Some(Token {
            term: word.to_lowercase(),
            span: start..end,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(text: &str) -> Vec<String> {
        tokens(text).map(|token| token.term).collect()
    }

    #[test]
    fn words_split_on_anything_but_letters_and_digits_and_fold_case() {
        assert_eq!(
            terms("Fix the `fold_entry` race, v2!"),
            ["fix", "the", "fold", "entry", "race", "v2"]
        );
    }

    #[test]
    fn spans_point_back_into_the_text_even_past_multibyte_letters() {
        let text = "Ça marche — déjà";
        let spans: Vec<&str> = tokens(text).map(|token| &text[token.span]).collect();
        assert_eq!(spans, ["Ça", "marche", "déjà"]);
        assert_eq!(terms(text), ["ça", "marche", "déjà"]);
    }

    #[test]
    fn empty_and_punctuation_only_text_has_no_tokens() {
        assert_eq!(terms(""), Vec::<String>::new());
        assert_eq!(terms(" -- ... "), Vec::<String>::new());
    }
}
