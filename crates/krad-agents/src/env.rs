use crate::cli::{DEFAULT_READ_LINES, DEFAULT_WAIT_TIMEOUT};
use crate::handle::Caller;
use crate::server::Endpoint;
use krad_domain::AgentPreset;
use std::ffi::OsString;
use std::time::Duration;

pub const ENV_ENDPOINT: &str = "KRAD_ADE_ENDPOINT";
pub const ENV_TOKEN: &str = "KRAD_ADE_TOKEN";
pub const ENV_TOKEN_FILE: &str = "KRAD_ADE_TOKEN_FILE";
pub const ENV_TERMINAL: &str = "KRAD_ADE_TERMINAL";
pub const CLI_PROGRAM: &str = krad_host::HOST_CLI_PROGRAM;
const WORKER_REPORT_WAIT: Duration = Duration::from_mins(15);
const LEAD_WAIT_TIMEOUT: Duration = Duration::from_mins(25);
const LEAD_READ_LINES: usize = 80;
const PROGRAM_SEPARATOR: &str = "|";

pub(crate) struct BusEnv {
    pub(crate) endpoint: String,
    pub(crate) token: String,
    pub(crate) terminal: String,
}

pub(crate) fn bus_env() -> Option<BusEnv> {
    bus_env_from(|name| std::env::var_os(name))
}

fn bus_env_from(var: impl Fn(&str) -> Option<OsString>) -> Option<BusEnv> {
    let text = |name: &str| var(name).and_then(|value| value.into_string().ok());
    let endpoint = text(ENV_ENDPOINT)?;
    let terminal = text(ENV_TERMINAL)?;
    let token = text(ENV_TOKEN).or_else(|| {
        var(ENV_TOKEN_FILE)
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|token| token.trim().to_owned())
            .filter(|token| !token.is_empty())
    })?;
    Some(BusEnv {
        endpoint,
        token,
        terminal,
    })
}

#[must_use]
pub fn member_token(endpoint: &Endpoint) -> &str {
    endpoint.token.as_str()
}

#[must_use]
pub fn token_file_env(
    address: &str,
    token_file: &std::path::Path,
    caller: &Caller,
) -> Vec<(String, String)> {
    [
        (ENV_ENDPOINT, address.to_owned()),
        (ENV_TOKEN_FILE, token_file.to_string_lossy().into_owned()),
        (ENV_TERMINAL, caller.to_string()),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value))
    .collect()
}

#[must_use]
pub fn terminal_env(endpoint: &Endpoint, caller: &Caller) -> Vec<(String, String)> {
    let token = match caller {
        Caller::Lead(_) => &endpoint.lead_token,
        Caller::Terminal(_) | Caller::Git(_) => &endpoint.token,
    };
    [
        (ENV_ENDPOINT, endpoint.address.to_string()),
        (ENV_TOKEN, token.as_str().to_owned()),
        (ENV_TERMINAL, caller.to_string()),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value))
    .collect()
}

#[must_use]
pub fn guide(cli: &str) -> String {
    let agents = agent_programs();
    let check_wait = DEFAULT_WAIT_TIMEOUT.as_secs();
    let report_wait = WORKER_REPORT_WAIT.as_secs();
    format!(
        "Krad ADE agent bus
You are running inside a Krad ADE terminal next to other coding agents.
Use this command to coordinate with them: {cli}

Commands:
  {cli} list
      List the other agents: terminal id, title, state and group.
  {cli} send <id> \"<message>\"
      Send a message to terminal <id>. Use it to report results or hand off work.
      Example: {cli} send 4 \"API done, tests pass. Your turn on the UI.\"
  {cli} check [--wait SECS]
      Read your unread messages. With --wait, block until one arrives.
      Example: {cli} check --wait {check_wait}
  {cli} wait <id> [--timeout SECS]
      Block until terminal <id> stops working. Do this before using its result.
      Example: {cli} wait 4
  {cli} read <id> [--lines N]
      Show the last lines of terminal <id>'s screen.
      Example: {cli} read 4 --lines {DEFAULT_READ_LINES}
  {cli} spawn --agent <{agents}> [--worktree <name>] \"<task>\"
      Start a worker agent in a session of its own in your group (or a new
      git worktree of it). It reports back to you with send when done; wait for
      it with {cli} check --wait {report_wait}.
  Add --json to any command for raw JSON output.

Messages may also arrive as a typed notice like
\"You have N new message(s) from other agents...\".
When you see it, run `{cli} check` to read them.
"
    )
}

#[must_use]
pub fn orchestrator_guide(cli: &str) -> String {
    let agents = agent_programs();
    let wait_timeout = LEAD_WAIT_TIMEOUT.as_secs();
    format!(
        "Krad ADE orchestration
You answer in the Krad ADE chat and can run coding agents as workers in the
user's group, with the shell command {cli}. Orchestrate only when the user asks
for work to be done (build, fix, implement, review, run a task); for questions,
just answer. You cannot edit files or run other commands yourself: delegate the
work to agents and check it by reading files and their screens.

Commands:
  {cli} list
      The agents of the environment: id, title, state and group.
  {cli} spawn --agent <{agents}> [--worktree <name>] \"<task>\"
      Start a worker in a session of its own in the group, as a chat, and
      print its id once it has started; if it cannot start, the error says
      why. Use --worktree for workers that edit the same repository in
      parallel.
  {cli} send <id> \"<instruction>\"
      Send worker <id> an instruction as its next message, as the user would.
      A closed worker reopens with its conversation to take it.
  {cli} wait <id> --timeout {wait_timeout}
      Block until worker <id> stops working. Always pass a timeout.
  {cli} read <id> --lines {LEAD_READ_LINES}
      The end of worker <id>'s conversation. Reading a worker that has
      finished closes it: read it once you are done waiting for it.

How to orchestrate:
  1. Split the work into self-contained tasks, each with acceptance criteria,
     and write them into the spawn task so the agent needs nothing else.
  2. Spawn one agent per task, then wait for each.
  3. When a worker stops, read its conversation and the files it changed, and
     check the acceptance criteria.
  4. If the work falls short or the worker is stuck, send a precise correction
     and wait again. If it waits for a permission, tell the user to answer it
     in the worker's session.
  5. Finish with a short report: what each agent did, what you verified and
     what is left for the user.
"
    )
}

fn agent_programs() -> String {
    AgentPreset::NATIVE
        .into_iter()
        .filter_map(AgentPreset::program)
        .collect::<Vec<_>>()
        .join(PROGRAM_SEPARATOR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn the_token_comes_from_its_file_when_it_is_not_in_the_environment() -> TestResult {
        let file = std::env::temp_dir().join(format!("krad-bus-token-{}", std::process::id()));
        std::fs::write(&file, "secret\n")?;
        let vars: HashMap<&str, OsString> = [
            (ENV_ENDPOINT, OsString::from("127.0.0.1:4000")),
            (ENV_TERMINAL, OsString::from("7")),
            (ENV_TOKEN_FILE, file.clone().into_os_string()),
        ]
        .into_iter()
        .collect();
        let env = bus_env_from(|name| vars.get(name).cloned()).ok_or("no bus env")?;
        std::fs::remove_file(&file)?;
        assert_eq!(env.token, "secret");
        assert_eq!(env.endpoint, "127.0.0.1:4000");
        assert!(bus_env_from(|name| (name == ENV_ENDPOINT).then(|| OsString::from("x"))).is_none());
        Ok(())
    }

    #[test]
    fn the_guides_name_every_agent_program_and_the_suggested_waits() {
        let member = guide(CLI_PROGRAM);
        for line in [
            "krad-ade-agents spawn --agent <claude|codex|opencode> [--worktree <name>]",
            "Example: krad-ade-agents check --wait 600",
            "Example: krad-ade-agents read 4 --lines 40",
            "it with krad-ade-agents check --wait 900.",
        ] {
            assert!(member.contains(line), "{line}");
        }
        let lead = orchestrator_guide(CLI_PROGRAM);
        for line in [
            "krad-ade-agents spawn --agent <claude|codex|opencode> [--worktree <name>]",
            "krad-ade-agents wait <id> --timeout 1500",
            "krad-ade-agents read <id> --lines 80",
        ] {
            assert!(lead.contains(line), "{line}");
        }
    }
}
