const SINGLE_QUOTE: char = '\'';
const DOUBLE_QUOTE: char = '"';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandLine {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quoting {
    Bare,
    Inside(char),
}

fn words(line: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut quoting = Quoting::Bare;
    for letter in line.chars() {
        match quoting {
            Quoting::Inside(quote) if letter == quote => quoting = Quoting::Bare,
            Quoting::Inside(_) => word.get_or_insert_default().push(letter),
            Quoting::Bare if letter == SINGLE_QUOTE || letter == DOUBLE_QUOTE => {
                quoting = Quoting::Inside(letter);
                word.get_or_insert_default();
            }
            Quoting::Bare if letter.is_whitespace() => words.extend(word.take()),
            Quoting::Bare => word.get_or_insert_default().push(letter),
        }
    }
    if quoting != Quoting::Bare {
        return None;
    }
    words.extend(word);
    Some(words)
}

impl CommandLine {
    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        let mut words = words(line)?.into_iter();
        let program = words.next()?;
        Some(Self {
            program,
            args: words.collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(line: &str) -> Option<(String, Vec<String>)> {
        CommandLine::parse(line).map(|command| (command.program, command.args))
    }

    #[test]
    fn words_split_on_spaces_outside_quotes() {
        assert_eq!(
            parsed("codex  login"),
            Some(("codex".to_owned(), vec!["login".to_owned()]))
        );
        assert_eq!(
            parsed(r#"my-agent --profile "work account" 'it''s' """#),
            Some((
                "my-agent".to_owned(),
                vec![
                    "--profile".to_owned(),
                    "work account".to_owned(),
                    "its".to_owned(),
                    String::new(),
                ]
            ))
        );
        assert_eq!(
            parsed(r#""C:\Program Files\agent\agent.exe" login"#),
            Some((
                r"C:\Program Files\agent\agent.exe".to_owned(),
                vec!["login".to_owned()]
            ))
        );
    }

    #[test]
    fn an_empty_line_or_an_open_quote_is_no_command() {
        assert_eq!(parsed("   "), None);
        assert_eq!(parsed(r#"agent "login"#), None);
    }
}
