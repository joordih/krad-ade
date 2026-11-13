#[cfg(unix)]
#[path = "support/cloud.rs"]
mod cloud;
#[cfg(unix)]
#[path = "cloud_chats/mirror.rs"]
mod mirror;
#[cfg(unix)]
#[path = "support/repo.rs"]
mod repo;
#[cfg(unix)]
#[path = "cloud_chats/runner.rs"]
mod runner;
#[cfg(unix)]
#[path = "cloud_chats/scenarios.rs"]
mod scenarios;

use std::io::Write as _;
use std::process::ExitCode;

type Case = (&'static str, fn() -> cloud::TestResult);

#[cfg(unix)]
const CASES: [Case; 2] = [
    (
        "a_cloud_chat_answers_on_the_runner_reattaches_from_its_offset_and_replays_its_journal",
        scenarios::a_cloud_chat_answers_on_the_runner_reattaches_from_its_offset_and_replays_its_journal,
    ),
    (
        "a_returning_runner_replays_what_its_chat_asked_while_the_app_was_away",
        scenarios::a_returning_runner_replays_what_its_chat_asked_while_the_app_was_away,
    ),
];

#[cfg(unix)]
fn main() -> ExitCode {
    if let Some(code) = runner::serve_as_host_cli() {
        return code;
    }
    let mut out = std::io::stdout().lock();
    let mut failed = 0_usize;
    for (name, case) in CASES {
        let outcome = match case() {
            Ok(()) => "ok".to_owned(),
            Err(error) => {
                failed += 1;
                format!("FAILED: {error}")
            }
        };
        drop(writeln!(out, "test {name} ... {outcome}"));
    }
    drop(writeln!(
        out,
        "test result: {} passed; {failed} failed",
        CASES.len() - failed
    ));
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(not(unix))]
fn main() -> ExitCode {
    drop(writeln!(std::io::stdout(), "cloud chats run on Unix only"));
    ExitCode::SUCCESS
}
