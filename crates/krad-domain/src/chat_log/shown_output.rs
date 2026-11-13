use serde::{Deserialize, Serialize};

use crate::chat::ToolOutput;

const SHOWN_LINES: usize = 12;
const LINE_BREAK: char = '\n';
const EXIT_CODE_LEAD: &str = "Exited with code";
const SIGNAL_LEAD: &str = "Stopped by";
const SUCCESS_CODE: u32 = 0;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShownOutput {
    pub text: String,
    pub failure: Option<String>,
}

impl From<&ToolOutput> for ShownOutput {
    fn from(output: &ToolOutput) -> Self {
        let trimmed = output.text.trim_end_matches(LINE_BREAK);
        let start = trimmed
            .rmatch_indices(LINE_BREAK)
            .nth(SHOWN_LINES - 1)
            .map_or(0, |(at, _)| at + 1);
        let failure = match (&output.signal, output.exit_code) {
            (Some(signal), _) => Some(format!("{SIGNAL_LEAD} {signal}")),
            (None, Some(code)) if code != SUCCESS_CODE => Some(format!("{EXIT_CODE_LEAD} {code}")),
            (None, _) => None,
        };
        Self {
            text: trimmed[start..].to_owned(),
            failure,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_last_lines_and_a_failed_exit_are_shown() {
        let text = (1..=20).fold(String::new(), |text, line| text + &line.to_string() + "\n");
        let shown = ShownOutput::from(&ToolOutput {
            text,
            truncated: false,
            exit_code: Some(2),
            signal: None,
        });
        assert_eq!(shown.text.lines().count(), SHOWN_LINES);
        assert!(shown.text.starts_with("9\n") && shown.text.ends_with("20"));
        assert_eq!(shown.failure.as_deref(), Some("Exited with code 2"));
    }

    #[test]
    fn a_short_successful_output_shows_whole() {
        let shown = ShownOutput::from(&ToolOutput {
            text: "hello\n".to_owned(),
            exit_code: Some(0),
            ..ToolOutput::default()
        });
        assert_eq!(shown.text, "hello");
        assert_eq!(shown.failure, None);
    }
}
