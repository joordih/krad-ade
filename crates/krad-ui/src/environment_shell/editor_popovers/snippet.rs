use std::collections::HashMap;

const DOLLAR: char = '$';
const OPEN: char = '{';
const CLOSE: char = '}';
const ESCAPE: char = '\\';
const DEFAULT_SEPARATOR: char = ':';
const CHOICE: char = '|';
const CHOICE_SEPARATOR: char = ',';
const FINAL_STOP: u32 = 0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PlainSnippet {
    pub(super) text: String,
    pub(super) cursor: usize,
}

fn read_number(chars: &[char], at: &mut usize) -> Option<u32> {
    let start = *at;
    while chars.get(*at).is_some_and(char::is_ascii_digit) {
        *at += 1;
    }
    chars[start..*at].iter().collect::<String>().parse().ok()
}

pub(super) fn plain_snippet(snippet: &str) -> PlainSnippet {
    let chars: Vec<char> = snippet.chars().collect();
    let mut text = String::new();
    let mut first_stop: Option<(u32, usize)> = None;
    let mut final_stop: Option<usize> = None;
    let mut defaults: HashMap<u32, String> = HashMap::new();
    let mut at = 0;
    let mark =
        |stop: u32, offset: usize, first: &mut Option<(u32, usize)>, last: &mut Option<usize>| {
            if stop == FINAL_STOP {
                last.get_or_insert(offset);
            } else if first.is_none_or(|(known, _)| stop < known) {
                *first = Some((stop, offset));
            }
        };
    while let Some(&ch) = chars.get(at) {
        if ch == ESCAPE
            && chars
                .get(at + 1)
                .is_some_and(|next| matches!(next, '$' | '}' | '\\'))
        {
            text.push(chars[at + 1]);
            at += 2;
            continue;
        }
        if ch != DOLLAR {
            text.push(ch);
            at += 1;
            continue;
        }
        let mut cursor = at + 1;
        if let Some(stop) = read_number(&chars, &mut cursor) {
            mark(stop, text.len(), &mut first_stop, &mut final_stop);
            if let Some(default) = defaults.get(&stop) {
                text.push_str(default);
            }
            at = cursor;
            continue;
        }
        if chars.get(cursor) != Some(&OPEN) {
            text.push(ch);
            at += 1;
            continue;
        }
        cursor += 1;
        let Some(stop) = read_number(&chars, &mut cursor) else {
            text.push(ch);
            at += 1;
            continue;
        };
        mark(stop, text.len(), &mut first_stop, &mut final_stop);
        let placeholder = match chars.get(cursor) {
            Some(&DEFAULT_SEPARATOR) => {
                let start = cursor + 1;
                let mut depth = 0usize;
                let mut end = start;
                while let Some(&inner) = chars.get(end) {
                    match inner {
                        OPEN => depth += 1,
                        CLOSE if depth == 0 => break,
                        CLOSE => depth -= 1,
                        _ => {}
                    }
                    end += 1;
                }
                cursor = end;
                plain_snippet(&chars[start..end].iter().collect::<String>()).text
            }
            Some(&CHOICE) => {
                let start = cursor + 1;
                let end = chars[start..]
                    .iter()
                    .position(|inner| *inner == CHOICE)
                    .map_or(chars.len(), |offset| start + offset);
                cursor = end + 1;
                chars[start..end]
                    .iter()
                    .collect::<String>()
                    .split(CHOICE_SEPARATOR)
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            }
            _ => String::new(),
        };
        text.push_str(&placeholder);
        defaults.entry(stop).or_insert(placeholder);
        while chars.get(cursor).is_some_and(|inner| *inner != CLOSE) {
            cursor += 1;
        }
        at = cursor + 1;
    }
    let cursor = first_stop
        .map(|(_, offset)| offset)
        .or(final_stop)
        .unwrap_or(text.len());
    PlainSnippet { text, cursor }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_keep_their_default_text_and_the_cursor_goes_to_the_first_stop() {
        assert_eq!(
            plain_snippet("println(${1:message})$0"),
            PlainSnippet {
                text: "println(message)".to_owned(),
                cursor: 8
            }
        );
        assert_eq!(
            plain_snippet("for (${1:int} ${2:i} = 0; $2 < n; $2++) {\n\t$0\n}"),
            PlainSnippet {
                text: "for (int i = 0; i < n; i++) {\n\t\n}".to_owned(),
                cursor: 5
            }
        );
    }

    #[test]
    fn choices_take_their_first_option_and_escapes_stay_literal() {
        assert_eq!(
            plain_snippet("${1|public,private|} void").text,
            "public void"
        );
        assert_eq!(plain_snippet(r"cost \$5").text, "cost $5");
        assert_eq!(
            plain_snippet("toString()"),
            PlainSnippet {
                text: "toString()".to_owned(),
                cursor: 10
            }
        );
        assert_eq!(plain_snippet("get($0)").cursor, 4);
    }
}
