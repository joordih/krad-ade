use krad_domain::{CloudProcessId, CloudProcessRun, JournalOffset};
use std::fmt;
use std::str::FromStr;

const FIELD_SEPARATOR: char = '\t';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudProcessStatus {
    pub id: CloudProcessId,
    pub run: CloudProcessRun,
    pub end: JournalOffset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a cloud status line is '<id>\\t<state>\\t<end>'")]
pub struct InvalidStatusLine;

impl fmt::Display for CloudProcessStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}{FIELD_SEPARATOR}{}{FIELD_SEPARATOR}{}",
            self.id, self.run, self.end.0
        )
    }
}

impl FromStr for CloudProcessStatus {
    type Err = InvalidStatusLine;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut fields = line.trim_end().split(FIELD_SEPARATOR);
        let (Some(id), Some(run), Some(end), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(InvalidStatusLine);
        };
        Ok(Self {
            id: id.parse().map_err(|_| InvalidStatusLine)?,
            run: run.parse().map_err(|_| InvalidStatusLine)?,
            end: JournalOffset(end.parse().map_err(|_| InvalidStatusLine)?),
        })
    }
}

/// # Errors
pub fn parse_status(output: &str) -> Result<Vec<CloudProcessStatus>, InvalidStatusLine> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::parse)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn status_lines_are_tab_separated_id_state_and_end() -> TestResult {
        let statuses = vec![
            CloudProcessStatus {
                id: "chat-1".parse()?,
                run: CloudProcessRun::Running,
                end: JournalOffset(12),
            },
            CloudProcessStatus {
                id: "shell-2".parse()?,
                run: CloudProcessRun::Ended(None),
                end: JournalOffset(0),
            },
        ];
        let output = statuses.iter().fold(String::new(), |mut text, status| {
            text.push_str(&status.to_string());
            text.push('\n');
            text
        });
        assert_eq!(output, "chat-1\trunning\t12\nshell-2\tended:?\t0\n");
        assert_eq!(parse_status(&output)?, statuses);
        assert_eq!(parse_status("")?, Vec::new());
        assert!(parse_status("chat-1\trunning").is_err());
        assert!(parse_status("chat-1\trunning\t1\textra").is_err());
        Ok(())
    }
}
