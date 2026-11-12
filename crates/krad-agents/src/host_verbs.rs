use krad_host::mux::{run_exec, serve, EXEC_VERB, SERVE_VERB};
use std::io::Write;
use std::time::Duration;

const EXIT_OK: i32 = 0;
const EXIT_FAILED: i32 = 1;
const HANDSHAKE_WAIT: Duration = Duration::from_secs(30);
const MISSING_ID: &str = "mux-exec needs a command id";

pub(crate) fn run(args: &[String]) -> Option<i32> {
    let (verb, rest) = args.split_first()?;
    match verb.as_str() {
        SERVE_VERB => Some(
            match serve(std::io::stdin(), std::io::stdout(), HANDSHAKE_WAIT, None) {
                Ok(()) => EXIT_OK,
                Err(error) => report(&error.to_string()),
            },
        ),
        EXEC_VERB => Some(match rest.first() {
            Some(id) => run_exec(id),
            None => report(MISSING_ID),
        }),
        _ => runner_verb(verb, rest),
    }
}

#[cfg(unix)]
fn runner_verb(verb: &str, rest: &[String]) -> Option<i32> {
    krad_host::runner::run_runner_verb(verb, rest)
}

#[cfg(not(unix))]
fn runner_verb(_verb: &str, _rest: &[String]) -> Option<i32> {
    None
}

fn report(message: &str) -> i32 {
    std::io::stderr()
        .write_all(format!("krad-ade-agents: {message}\n").as_bytes())
        .ok();
    EXIT_FAILED
}
