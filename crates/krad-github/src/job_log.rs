use krad_domain::review::{read_log_line, strip_ansi, JobLog, StepLines};
use serde::Deserialize;
use std::io::{self, BufRead};

use crate::model::Conclusion;

const STEP_HEADER: &str = "##[group]Run ";
const POST_STEPS: &str = "Post job cleanup.";
const ERROR_MARK: &str = "##[error]";
const TIMESTAMP_END: char = 'Z';
const TIMESTAMP_DATE_TIME: char = 'T';
const TIMESTAMP_SEPARATOR: char = ' ';
const TIMESTAMP_MIN_LEN: usize = 20;
const BYTE_ORDER_MARK: char = '\u{feff}';

#[derive(Deserialize)]
pub(crate) struct JobStepsJson {
    #[serde(default)]
    steps: Vec<StepJson>,
}

#[derive(Deserialize)]
struct StepJson {
    name: String,
    #[serde(default)]
    conclusion: Option<Conclusion>,
}

impl JobStepsJson {
    pub(crate) fn failing_step(&self) -> Option<&str> {
        self.steps
            .iter()
            .find(|step| step.conclusion.is_some_and(Conclusion::is_failure))
            .map(|step| step.name.as_str())
    }
}

pub(crate) fn read_job_log(
    mut log: impl BufRead,
    failing_step: Option<&str>,
) -> io::Result<JobLog> {
    let mut step = StepLines::default();
    let mut tail = StepLines::default();
    let mut failed = false;
    let mut raw = Vec::new();
    loop {
        if read_log_line(&mut log, &mut raw)? == 0 {
            break;
        }
        let line = String::from_utf8_lossy(&raw);
        let plain = strip_ansi(&line);
        let text = without_timestamp(plain.trim_start_matches(BYTE_ORDER_MARK).trim_end());
        if starts_step(text) {
            if failed {
                break;
            }
            step.restart();
        }
        failed |= text.starts_with(ERROR_MARK);
        step.push(text);
        tail.push(text);
    }
    Ok(JobLog::new(failing_step, if failed { step } else { tail }))
}

fn starts_step(text: &str) -> bool {
    text.starts_with(STEP_HEADER) || text == POST_STEPS
}

fn without_timestamp(line: &str) -> &str {
    let Some((stamp, text)) = line.split_once(TIMESTAMP_SEPARATOR) else {
        return if is_timestamp(line) { "" } else { line };
    };
    if is_timestamp(stamp) {
        text
    } else {
        line
    }
}

fn is_timestamp(word: &str) -> bool {
    word.len() >= TIMESTAMP_MIN_LEN
        && word.ends_with(TIMESTAMP_END)
        && word.contains(TIMESTAMP_DATE_TIME)
        && word.starts_with(|character: char| character.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOB: &str = include_str!("../fixtures/job.json");
    const LOG: &str = include_str!("../fixtures/job_log.txt");
    const EXCERPT: &str = include_str!("../fixtures/job_log_excerpt.txt");
    const PASSING_LOG: &str = "2024-05-01T10:00:00.0000000Z ##[group]Run cargo fmt\n\
         2024-05-01T10:00:01.0000000Z all formatted\n";

    #[test]
    fn the_first_failed_step_names_the_failure() -> serde_json::Result<()> {
        let job: JobStepsJson = serde_json::from_str(JOB)?;
        assert_eq!(job.failing_step(), Some("Run tests"));
        Ok(())
    }

    #[test]
    fn the_log_is_cut_to_the_failing_step_without_stamps_colors_or_secrets() -> io::Result<()> {
        let log = read_job_log(LOG.as_bytes(), Some("Run tests"))?;
        assert_eq!(log.failing_step(), Some("Run tests"));
        assert_eq!(log.lines(), EXCERPT.lines().collect::<Vec<_>>());
        assert_eq!(log.omitted(), 0);
        Ok(())
    }

    #[test]
    fn a_log_without_errors_keeps_its_tail() -> io::Result<()> {
        let log = read_job_log(PASSING_LOG.as_bytes(), None)?;
        assert_eq!(log.lines(), ["##[group]Run cargo fmt", "all formatted"]);
        Ok(())
    }
}
