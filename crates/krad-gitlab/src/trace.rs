use krad_domain::review::{read_log_line, strip_ansi, JobLog, StepLines};
use std::io::{self, BufRead};

const SECTION_START: &str = "section_start:";
const SECTION_END: &str = "section_end:";
const SECTION_FIELD_SEPARATOR: char = ':';
const SECTION_NAME_ENDS: [char; 2] = ['\r', '['];
const JOB_FAILED: &str = "ERROR: Job failed";
const AFTER_SCRIPT_SECTIONS: [&str; 7] = [
    "after_script",
    "archive_cache",
    "archive_cache_on_failure",
    "upload_artifacts_on_success",
    "upload_artifacts_on_failure",
    "cleanup_file_variables",
    "cleanup_build_dir",
];

pub(crate) fn read_trace(mut trace: impl BufRead) -> io::Result<JobLog> {
    let mut step = StepLines::default();
    let mut tail = StepLines::default();
    let mut failing: Option<String> = None;
    let mut script_over = false;
    let mut failed = false;
    let mut raw = Vec::new();
    loop {
        if read_log_line(&mut trace, &mut raw)? == 0 {
            break;
        }
        let line = String::from_utf8_lossy(&raw);
        let plain = strip_ansi(&line);
        let text = plain.trim_end();
        tail.push(text);
        if text.starts_with(SECTION_END) {
            continue;
        }
        if let Some(name) = section_name(text) {
            if AFTER_SCRIPT_SECTIONS.contains(&name) {
                script_over = true;
            } else if !script_over {
                failing = Some(name.to_owned());
                step.restart();
            }
        }
        if text.contains(JOB_FAILED) {
            failed = true;
            step.push(text);
            continue;
        }
        if !script_over {
            step.push(text);
        }
    }
    let lines = if failed && !step.is_empty() {
        step
    } else {
        failing = None;
        tail
    };
    Ok(JobLog::new(failing.as_deref(), lines))
}

fn section_name(text: &str) -> Option<&str> {
    let fields = text.strip_prefix(SECTION_START)?;
    let (_, rest) = fields.split_once(SECTION_FIELD_SEPARATOR)?;
    let name = rest.split(SECTION_NAME_ENDS).next()?;
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACE: &str = include_str!("../fixtures/job_trace.txt");
    const EXCERPT: &str = include_str!("../fixtures/job_trace_excerpt.txt");

    #[test]
    fn a_trace_is_cut_to_its_failing_section_and_the_failure() -> io::Result<()> {
        let log = read_trace(TRACE.as_bytes())?;
        assert_eq!(log.failing_step(), Some("step_script"));
        assert_eq!(log.lines(), EXCERPT.lines().collect::<Vec<_>>());
        Ok(())
    }

    #[test]
    fn a_trace_that_did_not_fail_keeps_its_tail() -> io::Result<()> {
        let log = read_trace("Running with gitlab-runner\nJob succeeded\n".as_bytes())?;
        assert_eq!(log.failing_step(), None);
        assert_eq!(log.lines(), ["Running with gitlab-runner", "Job succeeded"]);
        Ok(())
    }

    #[test]
    fn section_markers_name_their_section() {
        assert_eq!(
            section_name("section_start:1700000000:step_script\r"),
            Some("step_script")
        );
        assert_eq!(
            section_name("section_start:1700000000:build[collapsed=true]\rBuild"),
            Some("build")
        );
        assert_eq!(section_name("echo section_start"), None);
    }
}
