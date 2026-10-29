use crate::client::call;
use crate::env::{guide, orchestrator_guide, CLI_PROGRAM, ENV_ENDPOINT, ENV_TERMINAL, ENV_TOKEN};
use crate::handle::Caller;
use crate::hook::{run_ask, run_hook};
use crate::protocol::{AgentInfo, BusToken, Message, Request, Response, MAX_WAIT_SECS};
use crate::statusline::run_status_line;
use std::str::FromStr;

const EXIT_OK: i32 = 0;
const LEAD_TOPIC: &str = "lead";
const EXIT_USAGE: i32 = 1;
const EXIT_BUS: i32 = 2;

const DEFAULT_WAIT_TIMEOUT_SECS: u64 = 600;
const DEFAULT_READ_LINES: usize = 40;

const FLAG_JSON: &str = "--json";
const FLAG_WAIT: &str = "--wait";
const FLAG_TIMEOUT: &str = "--timeout";
const FLAG_LINES: &str = "--lines";
const FLAG_AGENT: &str = "--agent";
const FLAG_WORKTREE: &str = "--worktree";
const TERMINAL_ID: &str = "terminal id";

const NO_AGENTS: &str = "No agents";
const NO_MESSAGES: &str = "No new messages";
const TIMED_OUT: &str = "Timed out";
const OUTSIDE_TERMINAL: &str = "Run this inside a Krad ADE terminal.";
const SELF_MARKER: &str = "  (you)";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verb {
    List,
    Send,
    Check,
    Wait,
    Read,
    Spawn,
    Help,
    Hook,
    Ask,
    StatusLine,
}

impl Verb {
    const USAGE_ORDER: [Self; 7] = [
        Self::List,
        Self::Send,
        Self::Check,
        Self::Wait,
        Self::Read,
        Self::Spawn,
        Self::Help,
    ];
}

impl std::fmt::Display for Verb {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::List => "list",
            Self::Send => "send",
            Self::Check => "check",
            Self::Wait => "wait",
            Self::Read => "read",
            Self::Spawn => "spawn",
            Self::Help => "help",
            Self::Hook => "hook",
            Self::Ask => "ask",
            Self::StatusLine => "statusline",
        })
    }
}

impl FromStr for Verb {
    type Err = CliError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(match text {
            "list" => Self::List,
            "send" => Self::Send,
            "check" => Self::Check,
            "wait" => Self::Wait,
            "read" => Self::Read,
            "spawn" => Self::Spawn,
            "help" => Self::Help,
            "hook" => Self::Hook,
            "ask" => Self::Ask,
            "statusline" => Self::StatusLine,
            _ => return Err(CliError::UnknownCommand(text.to_owned())),
        })
    }
}

fn usage() -> String {
    let verbs = Verb::USAGE_ORDER
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("|");
    format!("Usage: krad-ade-agents <{verbs}> [args] [--json]")
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
enum CliError {
    #[error("send needs a terminal id and a message")]
    SendNeedsTarget,
    #[error("send needs a message")]
    SendNeedsMessage,
    #[error("spawn needs a task")]
    SpawnNeedsTask,
    #[error("spawn needs --agent <harness>")]
    SpawnNeedsAgent,
    #[error("missing terminal id")]
    MissingTerminal,
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("invalid {what}: `{text}`")]
    InvalidValue { what: String, text: String },
    #[error("{0} needs a value")]
    FlagNeedsValue(String),
}

enum Output {
    Stdout(String),
    Stderr(String),
}

#[derive(Default)]
struct Options {
    json: bool,
    wait: Option<u64>,
    timeout: Option<u64>,
    lines: Option<usize>,
    agent: Option<String>,
    worktree: Option<String>,
    positional: Vec<String>,
}

#[must_use]
pub fn run_cli(args: &[String]) -> i32 {
    let Some((verb, rest)) = args.split_first() else {
        emit(Output::Stdout(guide(CLI_PROGRAM)));
        return EXIT_OK;
    };
    let verb = match verb.parse::<Verb>() {
        Ok(verb) => verb,
        Err(error) => {
            emit(Output::Stderr(format!("{error}\n{}", usage())));
            return EXIT_USAGE;
        }
    };
    if verb == Verb::Hook {
        let Some((harness, hook_args)) = rest.split_first() else {
            return EXIT_OK;
        };
        return run_hook(harness, hook_args);
    }
    if verb == Verb::Ask {
        return rest.first().map_or(EXIT_OK, |harness| run_ask(harness));
    }
    if verb == Verb::StatusLine {
        return run_status_line(rest);
    }
    if verb == Verb::Help && rest.first().map(String::as_str) == Some(LEAD_TOPIC) {
        emit(Output::Stdout(orchestrator_guide(CLI_PROGRAM)));
        return EXIT_OK;
    }
    let parsed =
        parse_options(rest).and_then(|options| Ok((build_request(verb, &options)?, options.json)));
    match parsed {
        Ok((Some(request), json)) => execute(&request, json),
        Ok((None, _)) => {
            emit(Output::Stdout(guide(CLI_PROGRAM)));
            EXIT_OK
        }
        Err(error) => {
            emit(Output::Stderr(format!("{error}\n{}", usage())));
            EXIT_USAGE
        }
    }
}

fn execute(request: &Request, json: bool) -> i32 {
    let (Ok(endpoint), Ok(token), Ok(terminal)) = (
        std::env::var(ENV_ENDPOINT),
        std::env::var(ENV_TOKEN),
        std::env::var(ENV_TERMINAL),
    ) else {
        emit(Output::Stderr(OUTSIDE_TERMINAL.to_owned()));
        return EXIT_BUS;
    };
    let from = match terminal.parse::<Caller>() {
        Ok(from) => from,
        Err(error) => {
            emit(Output::Stderr(format!("{ENV_TERMINAL}: {error}")));
            return EXIT_BUS;
        }
    };
    match call(&endpoint, &BusToken::from(token), &from, request.clone()) {
        Ok(response) => report(request, &response, json),
        Err(error) => {
            emit(Output::Stderr(error.to_string()));
            EXIT_BUS
        }
    }
}

fn report(request: &Request, response: &Response, json: bool) -> i32 {
    let failed = matches!(response, Response::Error(_));
    let output = if json {
        match serde_json::to_string(response) {
            Ok(line) => Output::Stdout(line),
            Err(error) => Output::Stderr(error.to_string()),
        }
    } else if let Response::Error(message) = response {
        Output::Stderr(message.clone())
    } else {
        Output::Stdout(render(request, response))
    };
    emit(output);
    if failed {
        EXIT_BUS
    } else {
        EXIT_OK
    }
}

fn build_request(verb: Verb, options: &Options) -> Result<Option<Request>, CliError> {
    let request = match verb {
        Verb::Help | Verb::Hook | Verb::Ask | Verb::StatusLine => return Ok(None),
        Verb::List => Request::List,
        Verb::Send => {
            let (to, words) = options
                .positional
                .split_first()
                .ok_or(CliError::SendNeedsTarget)?;
            let body = words.join(" ");
            if body.is_empty() {
                return Err(CliError::SendNeedsMessage);
            }
            Request::Send {
                to: parse_value(TERMINAL_ID, to)?,
                body,
            }
        }
        Verb::Check => Request::Check {
            wait_secs: options.wait.map(|secs| secs.min(MAX_WAIT_SECS)),
        },
        Verb::Wait => Request::Wait {
            target: target(options)?,
            timeout_secs: options
                .timeout
                .unwrap_or(DEFAULT_WAIT_TIMEOUT_SECS)
                .min(MAX_WAIT_SECS),
        },
        Verb::Read => Request::Read {
            target: target(options)?,
            lines: options.lines.unwrap_or(DEFAULT_READ_LINES),
        },
        Verb::Spawn => {
            let task = options.positional.join(" ");
            if task.trim().is_empty() {
                return Err(CliError::SpawnNeedsTask);
            }
            Request::Spawn {
                agent: options.agent.clone().ok_or(CliError::SpawnNeedsAgent)?,
                worktree: options.worktree.clone(),
                task,
            }
        }
    };
    Ok(Some(request))
}

fn target(options: &Options) -> Result<u64, CliError> {
    let id = options
        .positional
        .first()
        .ok_or(CliError::MissingTerminal)?;
    parse_value(TERMINAL_ID, id)
}

fn parse_options(args: &[String]) -> Result<Options, CliError> {
    let mut options = Options::default();
    let mut words = args.iter();
    while let Some(word) = words.next() {
        let mut value = || {
            words
                .next()
                .ok_or_else(|| CliError::FlagNeedsValue(word.clone()))
        };
        match word.as_str() {
            FLAG_JSON => options.json = true,
            FLAG_AGENT => options.agent = Some(value()?.clone()),
            FLAG_WORKTREE => options.worktree = Some(value()?.clone()),
            FLAG_WAIT => options.wait = Some(parse_value(word, value()?)?),
            FLAG_TIMEOUT => options.timeout = Some(parse_value(word, value()?)?),
            FLAG_LINES => options.lines = Some(parse_value(word, value()?)?),
            _ => options.positional.push(word.clone()),
        }
    }
    Ok(options)
}

fn parse_value<T: FromStr>(what: &str, text: &str) -> Result<T, CliError> {
    text.parse().map_err(|_| CliError::InvalidValue {
        what: what.to_owned(),
        text: text.to_owned(),
    })
}

fn render(request: &Request, response: &Response) -> String {
    match response {
        Response::Agents(agents) if agents.is_empty() => NO_AGENTS.to_owned(),
        Response::Agents(agents) => lines(agents.iter().map(render_agent)),
        Response::Sent { id } => match request {
            Request::Send { to, .. } => format!("Sent message {id} to terminal {to}"),
            _ => format!("Sent message {id}"),
        },
        Response::Messages(messages) if messages.is_empty() => NO_MESSAGES.to_owned(),
        Response::Messages(messages) => lines(messages.iter().map(render_message)),
        Response::Settled { target, state } => format!("Terminal {target} is {state}"),
        Response::TimedOut => TIMED_OUT.to_owned(),
        Response::Screen(screen) => screen.clone(),
        Response::Noted | Response::Decided(_) => String::new(),
        Response::Spawned { terminal } => format!("Started worker in terminal {terminal}"),
        Response::Submitted { terminal } => format!("Typed the message into terminal {terminal}"),
        Response::Error(message) => message.clone(),
    }
}

fn render_agent(agent: &AgentInfo) -> String {
    let group = agent
        .group
        .as_ref()
        .map_or_default(|group| format!("  group: {group}"));
    let you = if agent.is_self { SELF_MARKER } else { "" };
    format!(
        "{}  {}  [{}]{group}{you}",
        agent.terminal, agent.title, agent.state
    )
}

fn render_message(message: &Message) -> String {
    format!(
        "#{} from {} ({}): {}",
        message.id, message.from, message.from_title, message.body
    )
}

fn lines(rendered: impl Iterator<Item = String>) -> String {
    rendered.collect::<Vec<_>>().join("\n")
}

#[allow(clippy::print_stdout, clippy::print_stderr)]
fn emit(output: Output) {
    match output {
        Output::Stdout(text) => println!("{text}"),
        Output::Stderr(text) => eprintln!("{text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_needs_an_agent_and_a_task() -> Result<(), CliError> {
        let mut options = parse_options(&["--agent", "claude"].map(str::to_owned))?;
        assert_eq!(
            build_request(Verb::Spawn, &options),
            Err(CliError::SpawnNeedsTask)
        );
        options.positional = vec!["fix".to_owned(), "it".to_owned()];
        assert_eq!(
            build_request(Verb::Spawn, &options)?,
            Some(Request::Spawn {
                agent: "claude".to_owned(),
                worktree: None,
                task: "fix it".to_owned(),
            })
        );
        assert_eq!(
            usage(),
            "Usage: krad-ade-agents <list|send|check|wait|read|spawn|help> [args] [--json]"
        );
        assert_eq!("send".parse::<Verb>(), Ok(Verb::Send));
        Ok(())
    }
}
