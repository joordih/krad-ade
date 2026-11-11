const QUOTE: char = '"';
const ESCAPE: char = '\\';
const SLASH: char = '/';
const STAR: char = '*';
const NEWLINE: char = '\n';
const COMMA: char = ',';
const CLOSERS: [char; 2] = ['}', ']'];

pub(super) fn plain_json(text: &str) -> String {
    without_trailing_commas(&without_comments(text))
}

fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            QUOTE => {
                out.push(ch);
                copy_string(&mut chars, &mut out);
            }
            SLASH if chars.peek() == Some(&SLASH) => {
                for skipped in chars.by_ref() {
                    if skipped == NEWLINE {
                        out.push(NEWLINE);
                        break;
                    }
                }
            }
            SLASH if chars.peek() == Some(&STAR) => {
                chars.next();
                let mut last = None;
                for skipped in chars.by_ref() {
                    if last == Some(STAR) && skipped == SLASH {
                        break;
                    }
                    last = Some(skipped);
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

fn without_trailing_commas(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            QUOTE => {
                out.push(ch);
                copy_string(&mut chars, &mut out);
            }
            COMMA => {
                let closes = chars
                    .clone()
                    .find(|next| !next.is_whitespace())
                    .is_some_and(|next| CLOSERS.contains(&next));
                if !closes {
                    out.push(ch);
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

fn copy_string(chars: &mut impl Iterator<Item = char>, out: &mut String) {
    let mut escaped = false;
    for ch in chars.by_ref() {
        out.push(ch);
        if escaped {
            escaped = false;
        } else if ch == ESCAPE {
            escaped = true;
        } else if ch == QUOTE {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn comments_and_trailing_commas_are_dropped_outside_strings() -> Result<(), serde_json::Error> {
        let text = r#"{
  // the agents
  "agent": { "docs": { "description": "Uses // and /* in text, ok", }, },
  /* block
     comment */
  "command": [1, 2,],
  "quote": "a \" , }"
}"#;
        let parsed: Value = serde_json::from_str(&plain_json(text))?;
        assert_eq!(
            parsed["agent"]["docs"]["description"],
            "Uses // and /* in text, ok"
        );
        assert_eq!(parsed["command"], serde_json::json!([1, 2]));
        assert_eq!(parsed["quote"], "a \" , }");
        Ok(())
    }

    #[test]
    fn plain_json_is_left_as_it_is() {
        let text = r#"{"a":[1,2],"b":"x"}"#;
        assert_eq!(plain_json(text), text);
    }
}
