use super::{Job, JobLog, MergeRequest};
use crate::CodeHost;

pub const SUMMARY_FILES_LIMIT: usize = 60;
const LOG_FENCE: &str = "```";
const UNKNOWN_STEP: &str = "not reported";
const LINE_BREAK: &str = "\n";

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ChangeStat {
    pub path: String,
    pub added: usize,
    pub removed: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct CiFailure<'a> {
    pub host: CodeHost,
    pub request: &'a MergeRequest,
    pub job: &'a Job,
    pub log: &'a JobLog,
    pub changes: &'a [ChangeStat],
}

const fn request_noun(host: CodeHost) -> &'static str {
    match host {
        CodeHost::Gitlab => "merge request",
        CodeHost::Github => "pull request",
    }
}

const fn request_sigil(host: CodeHost) -> char {
    match host {
        CodeHost::Gitlab => '!',
        CodeHost::Github => '#',
    }
}

#[must_use]
pub fn ci_fix_prompt(failure: &CiFailure<'_>) -> String {
    let CiFailure {
        host,
        request,
        job,
        log,
        changes,
    } = *failure;
    let noun = request_noun(host);
    let step = log.failing_step().unwrap_or(UNKNOWN_STEP);
    let mut lines = vec![format!(
        "The CI job \"{}\" failed on {noun} {}{} \"{}\" (branch {} into {}).",
        job.name,
        request_sigil(host),
        request.iid,
        request.title,
        request.source_branch,
        request.target_branch,
    )];
    if !job.stage.is_empty() {
        lines.push(format!("Stage: {}", job.stage));
    }
    lines.push(format!("Failing step: {step}"));
    lines.push(format!("Job: {}", job.web_url));
    lines.push(String::new());
    lines.extend(log_lines(log));
    lines.push(String::new());
    lines.extend(change_lines(noun, changes));
    lines.push(String::new());
    lines.push(format!(
        "Find the cause of the failure, fix it on branch {}, and run the failing check \
         locally to confirm it passes before you finish. Do not push.",
        request.source_branch,
    ));
    lines.push(String::new());
    lines.join(LINE_BREAK)
}

fn log_lines(log: &JobLog) -> impl Iterator<Item = String> + '_ {
    let shown = log.lines().len();
    let heading = match log.omitted() {
        0 => format!("Log of the failing step ({shown} lines):"),
        omitted => format!(
            "Last {shown} lines of the failing step's log ({omitted} earlier lines left out):"
        ),
    };
    [heading, LOG_FENCE.to_owned()]
        .into_iter()
        .chain(log.lines().iter().cloned())
        .chain(std::iter::once(LOG_FENCE.to_owned()))
}

fn change_lines(noun: &str, changes: &[ChangeStat]) -> Vec<String> {
    if changes.is_empty() {
        return vec![format!("The {noun}'s changed files could not be read.")];
    }
    let (added, removed) = changes.iter().fold((0, 0), |(added, removed), change| {
        (added + change.added, removed + change.removed)
    });
    let mut lines = vec![format!(
        "The {noun} changes {} files (+{added} -{removed}):",
        changes.len()
    )];
    lines.extend(
        changes
            .iter()
            .take(SUMMARY_FILES_LIMIT)
            .map(|change| format!("- {} (+{} -{})", change.path, change.added, change.removed)),
    );
    let hidden = changes.len().saturating_sub(SUMMARY_FILES_LIMIT);
    if hidden > 0 {
        lines.push(format!("- and {hidden} more files"));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::{
        CiStatus, HeadRepository, JobId, MergeRequestIid, MergeRequestState, StepLines, User,
        UserId,
    };

    const PROMPT_FIXTURE: &str = include_str!("../../fixtures/ci_fix_prompt.txt");

    fn request() -> MergeRequest {
        MergeRequest {
            iid: MergeRequestIid(42),
            title: "Parse durations".to_owned(),
            description: None,
            state: MergeRequestState::Opened,
            draft: false,
            author: User {
                id: UserId(1),
                username: "ada".to_owned(),
                name: "Ada".to_owned(),
            },
            reviewers: Vec::new(),
            source_branch: "parse-durations".to_owned(),
            head_repository: HeadRepository::Same,
            target_branch: "main".to_owned(),
            sha: None,
            detailed_merge_status: None,
            has_conflicts: false,
            user_notes_count: 0,
            web_url: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            head_pipeline: None,
            diff_refs: None,
        }
    }

    fn job() -> Job {
        Job {
            id: JobId(7),
            name: "test (ubuntu)".to_owned(),
            stage: "CI".to_owned(),
            status: CiStatus::Failed,
            web_url: "https://github.com/octo/app/actions/runs/1/job/7".to_owned(),
            duration: None,
        }
    }

    #[test]
    fn the_prompt_names_the_job_step_log_and_changes() {
        let mut lines = StepLines::default();
        lines.push("\u{1b}[31merror\u{1b}[0m: test durations::parses_minutes failed");
        lines.push("token=ghp_abcdefghijklmnopqrstuvwxyz0123");
        let log = JobLog::new(Some("Run cargo test"), lines);
        let changes = [ChangeStat {
            path: "src/duration.rs".to_owned(),
            added: 12,
            removed: 3,
        }];
        let (request, job) = (request(), job());
        let prompt = ci_fix_prompt(&CiFailure {
            host: CodeHost::Github,
            request: &request,
            job: &job,
            log: &log,
            changes: &changes,
        });
        assert_eq!(prompt, PROMPT_FIXTURE);
    }

    #[test]
    fn a_long_change_list_is_cut_and_a_missing_one_is_said() {
        let changes: Vec<ChangeStat> = (0..SUMMARY_FILES_LIMIT + 2)
            .map(|index| ChangeStat {
                path: format!("src/file_{index}.rs"),
                added: 1,
                removed: 0,
            })
            .collect();
        let (request, job, log) = (request(), job(), JobLog::default());
        let failure = CiFailure {
            host: CodeHost::Gitlab,
            request: &request,
            job: &job,
            log: &log,
            changes: &changes,
        };
        let prompt = ci_fix_prompt(&failure);
        assert!(prompt.contains("merge request !42"));
        assert!(prompt.contains("- and 2 more files"));
        assert!(prompt.contains("Failing step: not reported"));
        let none = ci_fix_prompt(&CiFailure {
            changes: &[],
            ..failure
        });
        assert!(none.contains("The merge request's changed files could not be read."));
    }
}
