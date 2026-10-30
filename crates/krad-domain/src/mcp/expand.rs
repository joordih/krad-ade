use std::borrow::Cow;

const OPEN: &str = "${";
const CLOSE: char = '}';
const DEFAULT_SEPARATOR: &str = ":-";

#[must_use]
pub fn expand_variables(text: &str, lookup: impl Fn(&str) -> Option<String>) -> Cow<'_, str> {
    if !text.contains(OPEN) {
        return Cow::Borrowed(text);
    }
    let mut expanded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        expanded.push_str(&rest[..start]);
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find(CLOSE) else {
            expanded.push_str(&rest[start..]);
            return Cow::Owned(expanded);
        };
        let inner = &after[..end];
        let (name, fallback) = inner
            .split_once(DEFAULT_SEPARATOR)
            .map_or((inner, None), |(name, fallback)| (name, Some(fallback)));
        match lookup(name).or_else(|| fallback.map(str::to_owned)) {
            Some(value) => expanded.push_str(&value),
            None => expanded.push_str(&rest[start..=start + OPEN.len() + end]),
        }
        rest = &after[end + 1..];
    }
    expanded.push_str(rest);
    Cow::Owned(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(name: &str) -> Option<String> {
        (name == "HOME").then(|| "/home/me".to_owned())
    }

    #[test]
    fn known_variables_and_defaults_are_filled_in() {
        assert_eq!(expand_variables("${HOME}/data", lookup), "/home/me/data");
        assert_eq!(expand_variables("${PORT:-3000}", lookup), "3000");
        assert_eq!(expand_variables("a ${HOME:-x} b", lookup), "a /home/me b");
    }

    #[test]
    fn unknown_or_unclosed_references_stay_as_written() {
        assert_eq!(expand_variables("${MISSING}", lookup), "${MISSING}");
        assert_eq!(expand_variables("${HOME", lookup), "${HOME");
        assert!(matches!(
            expand_variables("plain", lookup),
            Cow::Borrowed("plain")
        ));
    }
}
