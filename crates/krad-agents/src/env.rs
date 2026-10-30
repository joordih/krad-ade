use crate::cli::{DEFAULT_READ_LINES, DEFAULT_WAIT_TIMEOUT};
use crate::handle::Caller;
use crate::server::Endpoint;
use krad_domain::AgentPreset;
use std::time::Duration;

pub const ENV_ENDPOINT: &str = "KRAD_ADE_ENDPOINT";
pub const ENV_TOKEN: &str = "KRAD_ADE_TOKEN";
pub const ENV_TERMINAL: &str = "KRAD_ADE_TERMINAL";
pub const CLI_PROGRAM: &str = "krad-ade-agents";
const WORKER_REPORT_WAIT: Duration = Duration::from_mins(15);
const LEAD_WAIT_TIMEOUT: Duration = Duration::from_mins(25);
const LEAD_READ_LINES: usize = 80;
const PROGRAM_SEPARATOR: &str = "|";

#[must_use]
pub fn terminal_env(endpoint: &Endpoint, caller: &Caller) -> Vec<(String, String)> {
    let token = match caller {
        Caller::Lead(_) => &endpoint.lead_token,
        Caller::Terminal(_) => &endpoint.token,
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
      Start a worker agent in a new terminal of your group (or a new git
      worktree of it). It reports back to you with send when done; wait for
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
You answer in the Krad ADE chat and can run coding agents in terminals of the
user's group, with the shell command {cli}. Orchestrate only when the user asks
for work to be done (build, fix, implement, review, run a task); for questions,
just answer. You cannot edit files or run other commands yourself: delegate the
work to agents and check it by reading files and their screens.

Commands:
  {cli} list
      The terminals of the environment: id, title, state and group.
  {cli} spawn --agent <{agents}> [--worktree <name>] \"<task>\"
      Start an agent in a new terminal of the group and print its id. Use
      --worktree for agents that edit the same repository in parallel.
  {cli} send <id> \"<instruction>\"
      Type an instruction into agent <id> as a prompt, as the user would.
      Also use it to answer an agent that asks a question or for permission.
  {cli} wait <id> --timeout {wait_timeout}
      Block until agent <id> stops working. Always pass a timeout.
  {cli} read <id> --lines {LEAD_READ_LINES}
      The last lines of agent <id>'s screen.

How to orchestrate:
  1. Split the work into self-contained tasks, each with acceptance criteria,
     and write them into the spawn task so the agent needs nothing else.
  2. Spawn one agent per task, then wait for each.
  3. When an agent stops, read its screen and the files it changed, and check
     the acceptance criteria. If wait returns at once, the agent may not have
     started yet: read its screen before deciding.
  4. If the work falls short or the agent is stuck, send a precise correction
     and wait again. If it asks for input, answer it with send.
  5. Finish with a short report: what each agent did, what you verified and
     what is left for the user.
"
    )
}

fn agent_programs() -> String {
    AgentPreset::all()
        .into_iter()
        .filter_map(AgentPreset::program)
        .collect::<Vec<_>>()
        .join(PROGRAM_SEPARATOR)
}

#[cfg(test)]
mod tests {
    use super::*;

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
