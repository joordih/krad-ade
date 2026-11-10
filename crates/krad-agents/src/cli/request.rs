use super::api_info::TokenDisplay;
use super::memory::{chat_request, memory_request, share_request};
use super::variants::variants_request;
use super::verb::{AutomationAction, KraddieAction, SessionAction, Verb, CREATE, REMEMBER, RUN};
use crate::code::{CodeRequest, CodeTool};
use crate::extension_tools::tool_call_request;
use crate::protocol::{
    preset_by_bus_name, preset_by_display_name, CloseOrder, CloseTarget, FilePosition, Request,
    WhenWorking, WorktreeFate, MAX_WAIT,
};
use krad_domain::{AgentPreset, Switch};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

pub(crate) const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_mins(10);
pub(crate) const DEFAULT_READ_LINES: usize = 40;
const UNKNOWN_AGENT: &str =
    "Unknown agent; use a harness program such as claude, codex or opencode, or an ACP agent id";

const FLAG_JSON: &str = "--json";
const FLAG_WAIT: &str = "--wait";
const FLAG_TIMEOUT: &str = "--timeout";
const FLAG_LINES: &str = "--lines";
const FLAG_AGENT: &str = "--agent";
const FLAG_WORKTREE: &str = "--worktree";
const FLAG_GROUP: &str = "--group";
const FLAG_ALL: &str = "--all";
const FLAG_ALL_IDLE: &str = "--all-idle";
const FLAG_SESSION: &str = "--session";
const FLAG_FORCE: &str = "--force";
const FLAG_REMOVE_WORKTREE: &str = "--remove-worktree";
const FLAG_HARNESS: &str = "--harness";
const FLAG_TERMINAL: &str = "--terminal";
const FLAG_SINCE: &str = "--since";
const FLAG_STREAM: &str = "--stream";
const FLAG_SHOW_TOKEN: &str = "--show-token";
const SEQ: &str = "sequence number";
const UNKNOWN_HARNESS: &str =
    "Unknown harness; use a program such as claude, codex or opencode, or a name such as \"Claude Code\"";
const AGENT_ID: &str = "agent id";
const TERMINAL_ID: &str = "terminal id";
const SESSION_ID: &str = "session id";
const LOCATION_SEPARATOR: char = ':';
const CODE_USAGE: &str =
    "Usage: code <diagnostics|definition|references|hover|rename> FILE[:LINE:COLUMN] [NEW_NAME]";

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum CliError {
    #[error("send needs a terminal id and a message")]
    SendNeedsTarget,
    #[error("send needs a message")]
    SendNeedsMessage,
    #[error("spawn needs a task")]
    SpawnNeedsTask,
    #[error("spawn needs --agent <harness or ACP agent id>")]
    SpawnNeedsAgent,
    #[error("{UNKNOWN_AGENT}: `{0}`")]
    UnknownAgent(String),
    #[error("missing terminal id")]
    MissingTerminal,
    #[error("session needs create [NAME] [--group GROUP] or close <session id>")]
    SessionNeedsAction,
    #[error("missing session id")]
    MissingSession,
    #[error("close needs one of: an agent id, --session <session id> or --all-idle")]
    CloseNeedsTarget,
    #[error("worktree needs create <name>")]
    WorktreeNeedsName,
    #[error("open needs FILE[:LINE[:COLUMN]]")]
    OpenNeedsFile,
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("invalid {what}: `{text}`")]
    InvalidValue { what: String, text: String },
    #[error("{0} needs a value")]
    FlagNeedsValue(String),
    #[error(
        "agents needs list [--all] or watch [--group G] [--harness H] [--terminal ID]... [--since SEQ --stream ID]"
    )]
    AgentsNeedAction,
    #[error("api takes no argument, or info [--show-token]")]
    ApiNeedsAction,
    #[error("{UNKNOWN_HARNESS}: `{0}`")]
    UnknownHarness(String),
    #[error("kraddie needs list, run <name> [prompt], remember <name> <text>, memory [list|show] <name>, chat new <name>, share <reader> <read-from> or unshare <reader> <read-from>")]
    KraddieNeedsAction,
    #[error("kraddie chat needs new <name>")]
    KraddieChatNeedsNew,
    #[error("kraddie {0} needs the reading Kraddie and the Kraddie it reads from")]
    KraddieShareNeedsPair(&'static str),
    #[error("kraddie {0} needs the Kraddie's name")]
    KraddieNeedsName(&'static str),
    #[error("kraddie remember needs the text to remember after the Kraddie's name")]
    KraddieNeedsMemory,
    #[error("automation needs list, run <name> [prompt], enable <name> or disable <name>")]
    AutomationNeedsAction,
    #[error("automation {0} needs the automation's name, or <kraddie>/<name>")]
    AutomationNeedsName(&'static str),
    #[error("{0}")]
    Code(String),
    #[error("variants needs run, status [RUN] or pick <RUN> <VARIANT> [--apply] --yes")]
    VariantsNeedAction,
    #[error("variants run needs the task every variant works on")]
    VariantsNeedTask,
    #[error("variants run needs two to four --variant <harness> [--model M] [--instruction TEXT]")]
    VariantCount,
    #[error("{0} belongs after the --variant it applies to")]
    FlagBeforeVariant(String),
    #[error("variants pick needs a run id and a variant number")]
    PickNeedsVariant,
    #[error("picking a winner discards the other variants and removes their worktrees; add --yes")]
    PickNeedsConfirmation,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Options {
    pub(super) json: bool,
    wait: Option<u64>,
    timeout: Option<u64>,
    lines: Option<usize>,
    agent: Option<String>,
    worktree: Option<String>,
    pub(super) group: Option<String>,
    pub(super) harness: Option<String>,
    pub(super) terminals: Vec<u64>,
    pub(super) since: Option<u64>,
    pub(super) stream: Option<String>,
    pub(super) token: TokenDisplay,
    session: Option<u64>,
    pub(super) all: bool,
    all_idle: bool,
    when_working: WhenWorking,
    worktree_fate: WorktreeFate,
    pub(super) positional: Vec<String>,
}

pub(super) fn parse_options(args: &[String]) -> Result<Options, CliError> {
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
            FLAG_GROUP => options.group = Some(value()?.clone()),
            FLAG_HARNESS => options.harness = Some(value()?.clone()),
            FLAG_TERMINAL => options.terminals.push(parse_value(TERMINAL_ID, value()?)?),
            FLAG_SINCE => options.since = Some(parse_value(SEQ, value()?)?),
            FLAG_STREAM => options.stream = Some(value()?.clone()),
            FLAG_SHOW_TOKEN => options.token = TokenDisplay::Shown,
            FLAG_WAIT => options.wait = Some(parse_value(word, value()?)?),
            FLAG_TIMEOUT => options.timeout = Some(parse_value(word, value()?)?),
            FLAG_LINES => options.lines = Some(parse_value(word, value()?)?),
            FLAG_SESSION => options.session = Some(parse_value(SESSION_ID, value()?)?),
            FLAG_ALL => options.all = true,
            FLAG_ALL_IDLE => options.all_idle = true,
            FLAG_FORCE => options.when_working = WhenWorking::CancelTurn,
            FLAG_REMOVE_WORKTREE => options.worktree_fate = WorktreeFate::Remove,
            _ => options.positional.push(word.clone()),
        }
    }
    Ok(options)
}

pub(super) fn build_request(verb: Verb, options: &Options) -> Result<Option<Request>, CliError> {
    let request = match verb {
        Verb::Help | Verb::Hook | Verb::Ask | Verb::StatusLine | Verb::Mcp => return Ok(None),
        Verb::Code => code_request(options)?,
        Verb::Tools => Request::Tools,
        Verb::Tool => {
            let (name, arguments) =
                tool_call_request(&options.positional).map_err(CliError::Code)?;
            Request::ExtensionTool { name, arguments }
        }
        Verb::List if options.all => Request::ListAll,
        Verb::List | Verb::Status => Request::List,
        Verb::Agents => super::agents::agents_request(options)?,
        Verb::Close => Request::Close(close_order(options)?),
        Verb::Api => {
            super::agents::api_action(options)?;
            Request::Hello
        }
        Verb::Sessions => Request::Sessions,
        Verb::Session => session_request(options)?,
        Verb::Worktree => worktree_request(options)?,
        Verb::Kraddie => kraddie_request(options)?,
        Verb::Automation => automation_request(options)?,
        Verb::Open => open_request(options)?,
        Verb::Variants => variants_request(options)?,
        Verb::Changes | Verb::Diff => Request::Changes {
            session: options
                .positional
                .first()
                .map(|id| parse_value(SESSION_ID, id))
                .transpose()?,
        },
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
            wait_secs: options.wait.map(|secs| secs.min(MAX_WAIT.as_secs())),
        },
        Verb::Wait => Request::Wait {
            target: target(options)?,
            timeout_secs: options
                .timeout
                .unwrap_or(DEFAULT_WAIT_TIMEOUT.as_secs())
                .min(MAX_WAIT.as_secs()),
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
                agent: spawned_agent(options.agent.as_deref())?,
                worktree: options.worktree.clone(),
                task,
            }
        }
    };
    Ok(Some(request))
}

fn code_request(options: &Options) -> Result<Request, CliError> {
    let code_usage = || CliError::Code(CODE_USAGE.to_owned());
    let (tool, rest) = options.positional.split_first().ok_or_else(code_usage)?;
    let (target, name) = rest.split_first().ok_or_else(code_usage)?;
    let tool = tool
        .parse::<CodeTool>()
        .map_err(|error| CliError::Code(error.to_string()))?;
    let new_name = (!name.is_empty()).then(|| name.join(" "));
    CodeRequest::new(tool, target, new_name)
        .map(Request::Code)
        .map_err(|error| CliError::Code(error.to_string()))
}

fn session_request(options: &Options) -> Result<Request, CliError> {
    let (action, rest) = options
        .positional
        .split_first()
        .ok_or(CliError::SessionNeedsAction)?;
    Ok(match action.parse::<SessionAction>()? {
        SessionAction::Create => Request::CreateSession {
            name: (!rest.is_empty()).then(|| rest.join(" ")),
            group: options.group.clone(),
        },
        SessionAction::Close => Request::CloseSession {
            session: parse_value(SESSION_ID, rest.first().ok_or(CliError::MissingSession)?)?,
        },
    })
}

fn close_order(options: &Options) -> Result<CloseOrder, CliError> {
    let target = match (
        options.positional.as_slice(),
        options.session,
        options.all_idle,
    ) {
        ([id], None, false) => CloseTarget::Agent(parse_value(AGENT_ID, id)?),
        ([], Some(session), false) => CloseTarget::Session(session),
        ([], None, true) => CloseTarget::AllIdle,
        _ => return Err(CliError::CloseNeedsTarget),
    };
    Ok(CloseOrder {
        target,
        when_working: options.when_working,
        worktree: options.worktree_fate,
    })
}

fn kraddie_request(options: &Options) -> Result<Request, CliError> {
    let (action, rest) = options
        .positional
        .split_first()
        .ok_or(CliError::KraddieNeedsAction)?;
    Ok(match action.parse::<KraddieAction>()? {
        KraddieAction::List => Request::Kraddies,
        KraddieAction::Run => {
            let (name, prompt) = rest.split_first().ok_or(CliError::KraddieNeedsName(RUN))?;
            let prompt = prompt.join(" ");
            Request::RunKraddie {
                name: name.clone(),
                prompt: (!prompt.trim().is_empty()).then_some(prompt),
            }
        }
        KraddieAction::Remember => {
            let (name, text) = rest
                .split_first()
                .ok_or(CliError::KraddieNeedsName(REMEMBER))?;
            let text = text.join(" ");
            if text.trim().is_empty() {
                return Err(CliError::KraddieNeedsMemory);
            }
            Request::RememberKraddie {
                name: name.clone(),
                text,
            }
        }
        KraddieAction::Memory => memory_request(rest)?,
        KraddieAction::Chat => chat_request(rest)?,
        KraddieAction::Share => share_request(rest, Switch::On)?,
        KraddieAction::Unshare => share_request(rest, Switch::Off)?,
    })
}

fn automation_request(options: &Options) -> Result<Request, CliError> {
    let (action, rest) = options
        .positional
        .split_first()
        .ok_or(CliError::AutomationNeedsAction)?;
    let action = action.parse::<AutomationAction>()?;
    if action == AutomationAction::List {
        return Ok(Request::Automations);
    }
    let (name, prompt) = rest
        .split_first()
        .ok_or(CliError::AutomationNeedsName(action.verb()))?;
    let name = name.clone();
    Ok(match action {
        AutomationAction::List => Request::Automations,
        AutomationAction::Run => {
            let prompt = prompt.join(" ");
            Request::RunAutomation {
                name,
                prompt: (!prompt.trim().is_empty()).then_some(prompt),
            }
        }
        AutomationAction::Enable => Request::SwitchAutomation {
            name,
            switch: Switch::On,
        },
        AutomationAction::Disable => Request::SwitchAutomation {
            name,
            switch: Switch::Off,
        },
    })
}

fn worktree_request(options: &Options) -> Result<Request, CliError> {
    match options.positional.as_slice() {
        [action, name] if action == CREATE => Ok(Request::CreateWorktree { name: name.clone() }),
        _ => Err(CliError::WorktreeNeedsName),
    }
}

fn open_request(options: &Options) -> Result<Request, CliError> {
    let location = options.positional.join(" ");
    if location.trim().is_empty() {
        return Err(CliError::OpenNeedsFile);
    }
    let (path, position) = file_location(&location);
    Ok(Request::OpenFile { path, position })
}

pub(super) fn file_location(text: &str) -> (PathBuf, Option<FilePosition>) {
    let number = |part: &str| part.parse::<u32>().ok();
    if let Some((rest, column)) = text.rsplit_once(LOCATION_SEPARATOR) {
        if let Some((path, line)) = rest.rsplit_once(LOCATION_SEPARATOR) {
            if let (Some(line), Some(column)) = (number(line), number(column)) {
                let position = FilePosition {
                    line,
                    column: Some(column),
                };
                return (PathBuf::from(path), Some(position));
            }
        }
        if let Some(line) = number(column) {
            let position = FilePosition { line, column: None };
            return (PathBuf::from(rest), Some(position));
        }
    }
    (PathBuf::from(text), None)
}

pub(super) fn spawned_agent(name: Option<&str>) -> Result<AgentPreset, CliError> {
    let name = name.ok_or(CliError::SpawnNeedsAgent)?;
    preset_by_bus_name(name)
        .or_else(|| preset_by_display_name(name))
        .or_else(|| name.parse().ok().map(AgentPreset::Acp))
        .filter(|preset| preset.is_agent())
        .ok_or_else(|| CliError::UnknownAgent(name.to_owned()))
}

fn target(options: &Options) -> Result<u64, CliError> {
    let id = options
        .positional
        .first()
        .ok_or(CliError::MissingTerminal)?;
    parse_value(TERMINAL_ID, id)
}

pub(super) fn parse_value<T: FromStr>(what: &str, text: &str) -> Result<T, CliError> {
    text.parse().map_err(|_| CliError::InvalidValue {
        what: what.to_owned(),
        text: text.to_owned(),
    })
}
