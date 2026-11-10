use super::request::CliError;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Verb {
    List,
    Agents,
    Status,
    Send,
    Check,
    Wait,
    Read,
    Spawn,
    Close,
    Sessions,
    Session,
    Worktree,
    Open,
    Changes,
    Diff,
    Kraddie,
    Automation,
    Variants,
    Api,
    Help,
    Hook,
    Ask,
    StatusLine,
    Code,
    Tools,
    Tool,
    Mcp,
}

impl Verb {
    const ALL: [Self; 27] = [
        Self::List,
        Self::Agents,
        Self::Status,
        Self::Send,
        Self::Check,
        Self::Wait,
        Self::Read,
        Self::Spawn,
        Self::Close,
        Self::Sessions,
        Self::Session,
        Self::Worktree,
        Self::Open,
        Self::Changes,
        Self::Diff,
        Self::Kraddie,
        Self::Automation,
        Self::Variants,
        Self::Api,
        Self::Help,
        Self::Hook,
        Self::Ask,
        Self::StatusLine,
        Self::Code,
        Self::Tools,
        Self::Tool,
        Self::Mcp,
    ];

    const USAGE_ORDER: [Self; 23] = [
        Self::List,
        Self::Agents,
        Self::Status,
        Self::Send,
        Self::Check,
        Self::Wait,
        Self::Read,
        Self::Spawn,
        Self::Close,
        Self::Sessions,
        Self::Session,
        Self::Worktree,
        Self::Open,
        Self::Changes,
        Self::Diff,
        Self::Kraddie,
        Self::Automation,
        Self::Variants,
        Self::Code,
        Self::Tools,
        Self::Tool,
        Self::Api,
        Self::Help,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Agents => "agents",
            Self::Status => "status",
            Self::Send => "send",
            Self::Check => "check",
            Self::Wait => "wait",
            Self::Read => "read",
            Self::Spawn => "spawn",
            Self::Close => "close",
            Self::Sessions => "sessions",
            Self::Session => "session",
            Self::Worktree => "worktree",
            Self::Open => "open",
            Self::Changes => "changes",
            Self::Diff => "diff",
            Self::Kraddie => "kraddie",
            Self::Automation => "automation",
            Self::Variants => "variants",
            Self::Api => "api",
            Self::Help => "help",
            Self::Hook => "hook",
            Self::Ask => "ask",
            Self::StatusLine => "statusline",
            Self::Code => "code",
            Self::Tools => "tools",
            Self::Tool => "tool",
            Self::Mcp => "mcp",
        }
    }
}

impl fmt::Display for Verb {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl FromStr for Verb {
    type Err = CliError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text == LEGACY_KRADDIE_VERB {
            return Ok(Self::Kraddie);
        }
        Self::ALL
            .into_iter()
            .find(|verb| verb.name() == text)
            .ok_or_else(|| CliError::UnknownCommand(text.to_owned()))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SessionAction {
    Create,
    Close,
}

impl FromStr for SessionAction {
    type Err = CliError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            CREATE => Ok(Self::Create),
            CLOSE => Ok(Self::Close),
            _ => Err(CliError::SessionNeedsAction),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum AgentsAction {
    List,
    Watch,
}

impl FromStr for AgentsAction {
    type Err = CliError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            LIST => Ok(Self::List),
            WATCH => Ok(Self::Watch),
            _ => Err(CliError::AgentsNeedAction),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ApiAction {
    Version,
    Info,
}

impl FromStr for ApiAction {
    type Err = CliError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            INFO => Ok(Self::Info),
            _ => Err(CliError::ApiNeedsAction),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum KraddieAction {
    List,
    Run,
    Remember,
    Memory,
    Chat,
    Share,
    Unshare,
}

impl FromStr for KraddieAction {
    type Err = CliError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            LIST => Ok(Self::List),
            RUN => Ok(Self::Run),
            REMEMBER => Ok(Self::Remember),
            MEMORY => Ok(Self::Memory),
            CHAT => Ok(Self::Chat),
            SHARE => Ok(Self::Share),
            UNSHARE => Ok(Self::Unshare),
            _ => Err(CliError::KraddieNeedsAction),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum AutomationAction {
    List,
    Run,
    Enable,
    Disable,
}

impl AutomationAction {
    pub(super) const fn verb(self) -> &'static str {
        match self {
            Self::List => LIST,
            Self::Run => RUN,
            Self::Enable => ENABLE,
            Self::Disable => DISABLE,
        }
    }
}

impl FromStr for AutomationAction {
    type Err = CliError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            LIST => Ok(Self::List),
            RUN => Ok(Self::Run),
            ENABLE => Ok(Self::Enable),
            DISABLE => Ok(Self::Disable),
            _ => Err(CliError::AutomationNeedsAction),
        }
    }
}

pub(super) const CREATE: &str = "create";
const WATCH: &str = "watch";
const INFO: &str = "info";
const ENABLE: &str = "enable";
const DISABLE: &str = "disable";
pub(super) const LIST: &str = "list";
pub(super) const SHOW: &str = "show";
pub(super) const NEW: &str = "new";
pub(super) const CHAT: &str = "chat";
pub(super) const SHARE: &str = "share";
pub(super) const UNSHARE: &str = "unshare";
pub(super) const RUN: &str = "run";
pub(super) const REMEMBER: &str = "remember";
pub(super) const MEMORY: &str = "memory";
const CLOSE: &str = "close";
pub(super) const LEGACY_KRADDIE_VERB: &str = "pet";

pub(super) fn usage(program: &str) -> String {
    let verbs = Verb::USAGE_ORDER
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("|");
    format!("Usage: {program} <{verbs}> [args] [--json]")
}
