use crate::cli::{exit_codes_help, DEFAULT_READ_LINES, DEFAULT_WAIT_TIMEOUT};
use crate::discovery::ApiFile;
use crate::handle::Caller;
use crate::protocol::{BusToken, API_VERSION};
use crate::server::Endpoint;
use krad_domain::AgentPreset;
use std::ffi::OsString;
use std::time::Duration;

pub const ENV_ENDPOINT: &str = "KRAD_ADE_ENDPOINT";
pub const ENV_TOKEN: &str = "KRAD_ADE_TOKEN";
pub const ENV_TOKEN_FILE: &str = "KRAD_ADE_TOKEN_FILE";
pub const ENV_ENDPOINT_FILE: &str = "KRAD_ADE_ENDPOINT_FILE";
pub const ENV_TERMINAL: &str = "KRAD_ADE_TERMINAL";
pub const CLI_PROGRAM: &str = krad_host::HOST_CLI_PROGRAM;
const WORKER_REPORT_WAIT: Duration = Duration::from_mins(15);
const LEAD_WAIT_TIMEOUT: Duration = Duration::from_mins(25);
const LEAD_READ_LINES: usize = 80;
const PROGRAM_SEPARATOR: &str = "|";
const ACP_AGENT_CHOICE: &str = "acp-agent-id";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BusSource {
    Environment,
    ApiFile { path: std::path::PathBuf, pid: u32 },
}

#[derive(Debug)]
pub(crate) struct BusEnv {
    pub(crate) endpoint: String,
    pub(crate) token: BusToken,
    pub(crate) terminal: String,
    pub(crate) source: BusSource,
}

pub(crate) fn bus_env(api_file: Option<&ApiFile>) -> Option<BusEnv> {
    bus_env_from(|name| std::env::var_os(name)).or_else(|| api_file.and_then(discovered_env))
}

fn discovered_env(file: &ApiFile) -> Option<BusEnv> {
    let found = file.read().ok()??;
    Some(BusEnv {
        endpoint: found.endpoint.to_string(),
        token: found.token,
        terminal: Caller::Client.to_string(),
        source: BusSource::ApiFile {
            path: file.path.clone(),
            pid: found.pid,
        },
    })
}

fn file_text(path: OsString) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

fn bus_env_from(var: impl Fn(&str) -> Option<OsString>) -> Option<BusEnv> {
    let text = |name: &str| var(name).and_then(|value| value.into_string().ok());
    let endpoint = match var(ENV_ENDPOINT_FILE) {
        Some(path) => file_text(path)?,
        None => text(ENV_ENDPOINT)?,
    };
    let terminal = text(ENV_TERMINAL)?;
    let token = text(ENV_TOKEN).or_else(|| var(ENV_TOKEN_FILE).and_then(file_text))?;
    Some(BusEnv {
        endpoint,
        token: BusToken::from(token),
        terminal,
        source: BusSource::Environment,
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
pub fn endpoint_file_env(
    endpoint_file: &std::path::Path,
    token_file: &std::path::Path,
    caller: &Caller,
) -> Vec<(String, String)> {
    [
        (ENV_ENDPOINT_FILE, endpoint_file),
        (ENV_TOKEN_FILE, token_file),
    ]
    .into_iter()
    .map(|(name, path)| (name.to_owned(), path.to_string_lossy().into_owned()))
    .chain([(ENV_TERMINAL.to_owned(), caller.to_string())])
    .collect()
}

#[must_use]
pub fn terminal_env(endpoint: &Endpoint, caller: &Caller) -> Vec<(String, String)> {
    let token = match caller {
        Caller::Lead(_) => &endpoint.lead_token,
        Caller::Terminal(_) | Caller::Git(_) | Caller::Client => &endpoint.token,
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
    let exit_codes = exit_codes_help();
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
      An ACP agent starts by the id Settings shows for it; write acp:<id>
      when the id is also a harness program, as in acp:opencode.
  {cli} status
      The agents grouped by state: working, waiting-for-input, done, idle
      and exited.
  {cli} sessions
      List the sessions: session id, name, terminals and group.
  {cli} session create [NAME] [--group GROUP]
      Open a new session in your group, or in the group named GROUP.
  {cli} session close <session id>
      Close a session and the workers it holds.
  {cli} worktree create <name>
      Create a git worktree of your group's repository on a new branch and
      open it as a group of its own.
  {cli} open <file>[:LINE[:COLUMN]]
      Open a file of your group in the app's editor, relative to the group.
  {cli} changes [<session id>]
      The files changed in a session's group (yours by default).
  {cli} diff [<session id>]
      The patch of those changes against HEAD.
  {cli} kraddie list
      Your Kraddies: id, name, harness, schedule, next run and last outcome.
  {cli} kraddie run <name> [prompt]
      Wake a Kraddie now: it runs its manual automation, or its first one.
  {cli} kraddie remember <name> <text>
      Hand a Kraddie a keepsake, one short line it carries into every run.
  {cli} kraddie memory <name>
      The keepsakes a Kraddie carries, numbered, with how many are left.
  {cli} automation list
      The automations of every Kraddie: triggers, next run, last outcome and runs.
  {cli} automation run <name> [prompt]
      Run an automation now, with an optional prompt; write <kraddie>/<name>
      when two Kraddies have an automation of that name.
  {cli} automation enable|disable <name>
      Switch an automation on or off.
  {cli} variants run --variant <agent> [--model M] [--instruction TEXT] ... [--test \"TASK\"] \"<task>\"
      Launch the same task as two to four variants, each in its own worktree,
      to compare in the app's Variants view. TASK is a project task such as
      \"cargo test\", run in each worktree once its variant finishes.
  {cli} variants status [<run id>]
      Each variant's state, cost, tokens, time, tests and diff.
  {cli} variants pick <run id> <variant> [--apply] --yes
      Keep the winner's worktree as its session (or, with --apply, apply its
      changes onto the base worktree) and discard the other variants.
  {cli} agents list [--all]
      Every agent with its terminal id, title, group, harness and state.
  {cli} agents watch [--group G] [--harness H] [--terminal ID]... [--since SEQ --stream ID]
      Print the agents, then one line each time one opens, changes state or
      closes, until interrupted. Repeat --terminal to follow only those ids. To resume, pass the last seq seen and the
      stream id from the first line; without the stream id the app answers
      with a fresh list.
  {cli} api
      The agents API version the app serves (this CLI speaks {API_VERSION}).
  {cli} api info [--show-token]
      Where the agents API listens, found through this terminal or through
      the app's api.json from any other terminal.
  Add --json to any command for raw JSON output.

Exit codes:
{exit_codes}

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
  {cli} list [--all]
      The agents of the environment: id, title, state and group. With --all,
      also the workers that were closed, as closed.
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
      finished closes it as close does: read it once you are done waiting.
  {cli} close <id> [--force] [--remove-worktree]
      Close worker <id>: its chat stops and its session leaves the sidebar.
      Its worktree and files stay on disk and the answer says where;
      --remove-worktree also removes the worktree, only when it has no
      uncommitted changes and no commits missing from the repository's
      checked-out branch. A worker still working is refused unless --force,
      which stops its turn first. Only workers you started can be closed.
  {cli} close --session <session id> [--force] [--remove-worktree]
      Close another session of your group the same way; never your own.
  {cli} close --all-idle [--remove-worktree]
      Close every worker of yours that has finished, and print their ids.

How to orchestrate:
  1. Split the work into self-contained tasks, each with acceptance criteria,
     and write them into the spawn task so the agent needs nothing else.
  2. Spawn one agent per task, then wait for each.
  3. When a worker stops, read its conversation and the files it changed, and
     check the acceptance criteria.
  4. If the work falls short or the worker is stuck, send a precise correction
     and wait again. If it waits for a permission, tell the user to answer it
     in the worker's session.
  5. Close each worker once its work is checked (reading a finished one
     closes it; close --all-idle closes the rest).
  6. Finish with a short report: what each agent did, what you verified and
     what is left for the user.
"
    )
}

fn agent_programs() -> String {
    AgentPreset::NATIVE
        .into_iter()
        .filter_map(AgentPreset::program)
        .chain([ACP_AGENT_CHOICE])
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
        assert_eq!(env.token, BusToken::from("secret"));
        assert_eq!(env.source, BusSource::Environment);
        assert_eq!(env.endpoint, "127.0.0.1:4000");
        assert!(bus_env_from(|name| (name == ENV_ENDPOINT).then(|| OsString::from("x"))).is_none());
        Ok(())
    }

    #[test]
    fn a_cloud_agent_reads_the_current_endpoint_from_its_file_on_every_call() -> TestResult {
        let dir = std::env::temp_dir().join(format!("krad-bus-endpoint-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let (endpoint, token) = (dir.join("runner-3.endpoint"), dir.join("runner-3.token"));
        std::fs::write(&token, "secret")?;
        let caller = Caller::Terminal(crate::handle::TerminalHandle {
            environment: "api".to_owned(),
            terminal: 4,
        });
        let vars: HashMap<String, OsString> = endpoint_file_env(&endpoint, &token, &caller)
            .into_iter()
            .map(|(name, value)| (name, OsString::from(value)))
            .collect();
        let resolve = || bus_env_from(|name| vars.get(name).cloned());
        assert!(resolve().is_none());
        std::fs::write(&endpoint, "127.0.0.1:41000\n")?;
        assert_eq!(resolve().ok_or("no bus env")?.endpoint, "127.0.0.1:41000");
        std::fs::write(&endpoint, "127.0.0.1:42017")?;
        let moved = resolve().ok_or("no bus env")?;
        std::fs::remove_dir_all(&dir)?;
        assert_eq!(moved.endpoint, "127.0.0.1:42017");
        assert_eq!(moved.token, BusToken::from("secret"));
        assert_eq!(moved.terminal, "api:4");
        Ok(())
    }

    #[test]
    fn the_guides_name_every_agent_program_and_the_suggested_waits() {
        let member = guide(CLI_PROGRAM);
        for line in [
            "krad-ade-agents spawn --agent <claude|codex|opencode|acp-agent-id> [--worktree <name>]",
            "Example: krad-ade-agents check --wait 600",
            "Example: krad-ade-agents read 4 --lines 40",
            "it with krad-ade-agents check --wait 900.",
            "krad-ade-agents session create [NAME] [--group GROUP]",
            "krad-ade-agents open <file>[:LINE[:COLUMN]]",
            "(this CLI speaks v1)",
            "krad-ade-agents agents watch [--group G] [--harness H] [--terminal ID]... [--since SEQ --stream ID]",
            "krad-ade-agents api info [--show-token]",
            "Repeat --terminal to follow only those ids.",
            "  6  the app speaks another API version",
        ] {
            assert!(member.contains(line), "{line}");
        }
        let lead = orchestrator_guide(CLI_PROGRAM);
        for line in [
            "krad-ade-agents spawn --agent <claude|codex|opencode|acp-agent-id> [--worktree <name>]",
            "krad-ade-agents wait <id> --timeout 1500",
            "krad-ade-agents read <id> --lines 80",
            "krad-ade-agents list [--all]",
            "krad-ade-agents close <id> [--force] [--remove-worktree]",
            "krad-ade-agents close --session <session id> [--force] [--remove-worktree]",
            "krad-ade-agents close --all-idle [--remove-worktree]",
        ] {
            assert!(lead.contains(line), "{line}");
        }
    }
}
