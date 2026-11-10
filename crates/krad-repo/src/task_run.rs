use krad_domain::{ProjectTask, TestOutcome, TestVerdict, MAX_TEST_TAIL_BYTES};
use krad_host::{HostCommand, HostError, HostLink};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub const TASK_RUN_TIMEOUT: Duration = Duration::from_mins(30);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);
const TOKEN_PREFIX: &str = "krad-task";
const CLEANUP_FAILED: &str = "; its output file could not be removed: ";
const KEEP_TAIL_SCRIPT: &str = r#"limit=$1; token=$2; shift 2; out=$(mktemp "${TMPDIR:-/tmp}/$token.XXXXXX") || exit 125; trap 'rm -f "$out"; exit 129' HUP INT TERM; "$@" >"$out" 2>&1; status=$?; tail -c "$limit" "$out"; rm -f "$out"; exit "$status""#;
const CLEANUP_SCRIPT: &str = r#"rm -f "${TMPDIR:-/tmp}/$1".*"#;

static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(0);

fn output_token() -> String {
    let serial = TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{TOKEN_PREFIX}-{}-{serial}", std::process::id())
}

#[must_use]
pub fn run_task(
    host: &HostLink,
    root: &Path,
    task: &ProjectTask,
    timeout: Duration,
) -> TestOutcome {
    run_task_with_token(host, root, task, timeout, &output_token())
}

fn remove_output(host: &HostLink, root: &Path, token: &str) -> Result<(), HostError> {
    let mut command = HostCommand::posix_shell(CLEANUP_SCRIPT);
    command.arg(token).current_dir(root).in_login_shell();
    host.output_within(&command, CLEANUP_TIMEOUT).map(drop)
}

fn run_task_with_token(
    host: &HostLink,
    root: &Path,
    task: &ProjectTask,
    timeout: Duration,
    token: &str,
) -> TestOutcome {
    let mut command = HostCommand::posix_shell(KEEP_TAIL_SCRIPT);
    command
        .arg(MAX_TEST_TAIL_BYTES.to_string())
        .arg(token)
        .arg(&task.program)
        .args(&task.args)
        .current_dir(root)
        .in_login_shell();
    let started = Instant::now();
    let output = host.output_within(&command, timeout);
    let took = started.elapsed();
    match output {
        Ok(output) => {
            let verdict = if output.status.success() {
                TestVerdict::Passed
            } else {
                TestVerdict::Failed
            };
            TestOutcome::new(verdict, took, &String::from_utf8_lossy(&output.stdout))
        }
        Err(error) => {
            let reason = match remove_output(host, root, token) {
                Ok(()) => error.to_string(),
                Err(cleanup) => format!("{error}{CLEANUP_FAILED}{cleanup}"),
            };
            TestOutcome::new(TestVerdict::CouldNotRun, took, &reason)
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use krad_domain::TaskSource;

    fn task(program: &str, args: &[&str]) -> ProjectTask {
        ProjectTask {
            source: TaskSource::Make,
            name: program.to_owned(),
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }

    #[test]
    fn a_task_passes_or_fails_by_its_exit_status_and_keeps_only_the_tail() {
        let root = std::env::temp_dir();
        let host = HostLink::Local;
        let passed = run_task(&host, &root, &task("true", &[]), TASK_RUN_TIMEOUT);
        assert_eq!(passed.verdict, TestVerdict::Passed);
        let loud = "head -c 50000 /dev/zero | tr '\\0' x; echo end; exit 3";
        let failed = run_task(&host, &root, &task("sh", &["-c", loud]), TASK_RUN_TIMEOUT);
        assert_eq!(failed.verdict, TestVerdict::Failed);
        assert!(failed.tail.len() <= MAX_TEST_TAIL_BYTES);
        assert!(failed.tail.ends_with("end\n"));
    }

    fn leftovers(token: &str) -> std::io::Result<usize> {
        let prefix = format!("{token}.");
        Ok(std::fs::read_dir(std::env::temp_dir())?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
            .count())
    }

    #[test]
    fn a_task_that_runs_too_long_could_not_run_and_leaves_no_output_file() -> std::io::Result<()> {
        let slow = task("sleep", &["30"]);
        let token = format!("{TOKEN_PREFIX}-test-timeout-{}", std::process::id());
        let outcome = run_task_with_token(
            &HostLink::Local,
            &std::env::temp_dir(),
            &slow,
            Duration::from_millis(300),
            &token,
        );
        assert_eq!(outcome.verdict, TestVerdict::CouldNotRun);
        assert_eq!(leftovers(&token)?, 0);
        Ok(())
    }
}
