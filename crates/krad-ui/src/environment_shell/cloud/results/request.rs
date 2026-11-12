use krad_domain::review::NewMergeRequest;
use krad_host::{HostCommand, HostLink};
use std::path::Path;
use std::time::Duration;

use super::super::workspace::git;

const GIT_WAIT: Duration = Duration::from_secs(60);
const ORIGIN_PREFIX: &str = "origin/";
const ORIGIN_HEAD: &str = "refs/remotes/origin/HEAD";
const SUBJECT_FORMAT: &str = "--format=%s";
const COMMIT_BULLET: &str = "- ";
const BODY_INTRO: &str = "Opened from a krad-ade cloud session.";
const COMMITS_HEADING: &str = "Commits:";
const LINE_BREAK: &str = "\n";
const PARAGRAPH_BREAK: &str = "\n\n";

fn stdout_of(link: &HostLink, command: &HostCommand) -> Option<String> {
    let output = link
        .output_within(command, GIT_WAIT)
        .inspect_err(|error| tracing::debug!(%error, "a git query on the runner failed"))
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub(super) fn default_branch(link: &HostLink, repo: &Path) -> Option<String> {
    let mut command = git(Some(repo));
    command.args(["symbolic-ref", "--short", ORIGIN_HEAD]);
    let named = stdout_of(link, &command)?;
    branch_of_remote_head(&named).map(str::to_owned)
}

fn branch_of_remote_head(named: &str) -> Option<&str> {
    named
        .strip_prefix(ORIGIN_PREFIX)
        .filter(|branch| !branch.is_empty())
}

pub(super) fn commit_subjects(link: &HostLink, worktree: &Path, target: &str) -> Vec<String> {
    let mut command = git(Some(worktree));
    command.args([
        "log",
        "--no-merges",
        SUBJECT_FORMAT,
        &format!("{ORIGIN_PREFIX}{target}..HEAD"),
    ]);
    stdout_of(link, &command).map_or_else(Vec::new, |text| {
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .collect()
    })
}

pub(super) fn new_request(
    title: &str,
    source: &str,
    target: &str,
    subjects: &[String],
) -> NewMergeRequest {
    let mut description = BODY_INTRO.to_owned();
    if !subjects.is_empty() {
        description.push_str(PARAGRAPH_BREAK);
        description.push_str(COMMITS_HEADING);
        for subject in subjects {
            description.push_str(LINE_BREAK);
            description.push_str(COMMIT_BULLET);
            description.push_str(subject);
        }
    }
    NewMergeRequest {
        source_branch: source.to_owned(),
        target_branch: target.to_owned(),
        title: title.to_owned(),
        description,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_is_titled_after_the_session_and_lists_its_commits() {
        let subjects = ["Fix the login".to_owned(), "Add a test".to_owned()];
        let request = new_request("Cloud · api", "krad/api-a1", "main", &subjects);
        assert_eq!(request.title, "Cloud · api");
        assert_eq!(request.source_branch, "krad/api-a1");
        assert_eq!(request.target_branch, "main");
        assert_eq!(
            request.description,
            "Opened from a krad-ade cloud session.\n\nCommits:\n- Fix the login\n- Add a test"
        );
        let bare = new_request("x", "krad/x", "main", &[]);
        assert_eq!(bare.description, BODY_INTRO);
    }

    #[test]
    fn the_default_branch_comes_from_the_remote_head() {
        assert_eq!(branch_of_remote_head("origin/main"), Some("main"));
        assert_eq!(branch_of_remote_head("origin/"), None);
        assert_eq!(branch_of_remote_head("main"), None);
    }
}
