mod commands;

use krad_domain::PermissionMode;
use std::collections::HashSet;
use std::path::Path;

use super::tools::{Action, WebUrl, Workspace};
use commands::{is_dangerous, parse, segment_is_safe, segment_key};

pub(crate) use commands::is_known_safe;

const PLAN_FILES_TEXT: &str = "Plan mode does not change files";
const PLAN_COMMANDS_TEXT: &str = "Plan mode only runs read-only commands";
const DONT_ASK_TEXT: &str = "This chat does not ask for approvals, and this action needs one";
const DONT_ASK_HOST_TEXT: &str =
    "This chat does not ask for approvals, and fetching from this host needs one";
const PLAN_MCP_TEXT: &str = "Plan mode does not run MCP tools";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Allow,
    Ask,
    Deny(&'static str),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Allowed {
    edits: bool,
    commands: HashSet<String>,
    hosts: HashSet<String>,
    mcp_tools: HashSet<String>,
}

impl Allowed {
    pub(crate) fn remember(&mut self, action: &Action) {
        match action {
            Action::Write { .. } | Action::Edit { .. } => self.edits = true,
            Action::Shell { command, .. } if !is_dangerous(command) => {
                self.commands
                    .extend(parse(command).segments.iter().filter_map(segment_key));
            }
            Action::WebFetch { url, .. } => {
                self.hosts.insert(url.host().to_owned());
            }
            Action::Mcp { name, .. } => {
                self.mcp_tools.insert(name.clone());
            }
            Action::Shell { .. }
            | Action::ShellOutput { .. }
            | Action::ShellStop { .. }
            | Action::WebSearch { .. }
            | Action::Read { .. }
            | Action::Find { .. }
            | Action::Grep { .. } => {}
        }
    }

    fn knows(&self, command: &str) -> bool {
        let parsed = parse(command);
        parsed.is_plain()
            && !parsed.segments.is_empty()
            && parsed.segments.iter().all(|segment| {
                segment_is_safe(&segment.words)
                    || segment_key(segment).is_some_and(|key| self.commands.contains(&key))
            })
    }
}

const fn refusal(mode: PermissionMode) -> Option<&'static str> {
    match mode {
        PermissionMode::Plan => Some(PLAN_FILES_TEXT),
        PermissionMode::DontAsk => Some(DONT_ASK_TEXT),
        PermissionMode::Manual
        | PermissionMode::AcceptEdits
        | PermissionMode::Auto
        | PermissionMode::BypassPermissions => None,
    }
}

fn file_verdict(
    path: &Path,
    mode: PermissionMode,
    workspace: &Workspace,
    allowed: &Allowed,
) -> Verdict {
    if mode == PermissionMode::BypassPermissions {
        return Verdict::Allow;
    }
    if let Some(reason) = refusal(mode) {
        return Verdict::Deny(reason);
    }
    if !workspace.contains(path) {
        return Verdict::Ask;
    }
    match mode {
        PermissionMode::Manual if !allowed.edits => Verdict::Ask,
        _ => Verdict::Allow,
    }
}

fn shell_verdict(command: &str, mode: PermissionMode, allowed: &Allowed) -> Verdict {
    if is_known_safe(command) {
        return Verdict::Allow;
    }
    let dangerous = is_dangerous(command);
    match mode {
        PermissionMode::BypassPermissions => Verdict::Allow,
        PermissionMode::Auto if !dangerous => Verdict::Allow,
        PermissionMode::Manual | PermissionMode::AcceptEdits
            if !dangerous && allowed.knows(command) =>
        {
            Verdict::Allow
        }
        PermissionMode::Auto | PermissionMode::Manual | PermissionMode::AcceptEdits => Verdict::Ask,
        PermissionMode::Plan => Verdict::Deny(PLAN_COMMANDS_TEXT),
        PermissionMode::DontAsk => Verdict::Deny(DONT_ASK_TEXT),
    }
}

fn fetch_verdict(url: &WebUrl, mode: PermissionMode, allowed: &Allowed) -> Verdict {
    if mode == PermissionMode::BypassPermissions {
        return Verdict::Allow;
    }
    if url.is_private() {
        return if mode == PermissionMode::DontAsk {
            Verdict::Deny(DONT_ASK_HOST_TEXT)
        } else {
            Verdict::Ask
        };
    }
    let remembered = allowed.hosts.contains(url.host());
    match mode {
        PermissionMode::Auto | PermissionMode::Plan | PermissionMode::BypassPermissions => {
            Verdict::Allow
        }
        PermissionMode::Manual | PermissionMode::AcceptEdits | PermissionMode::DontAsk
            if remembered =>
        {
            Verdict::Allow
        }
        PermissionMode::Manual | PermissionMode::AcceptEdits => Verdict::Ask,
        PermissionMode::DontAsk => Verdict::Deny(DONT_ASK_HOST_TEXT),
    }
}

fn mcp_verdict(name: &str, mode: PermissionMode, allowed: &Allowed) -> Verdict {
    match mode {
        PermissionMode::Auto | PermissionMode::BypassPermissions => Verdict::Allow,
        PermissionMode::Manual | PermissionMode::AcceptEdits
            if allowed.mcp_tools.contains(name) =>
        {
            Verdict::Allow
        }
        PermissionMode::Manual | PermissionMode::AcceptEdits => Verdict::Ask,
        PermissionMode::Plan => Verdict::Deny(PLAN_MCP_TEXT),
        PermissionMode::DontAsk => Verdict::Deny(DONT_ASK_TEXT),
    }
}

pub(crate) fn verdict(
    action: &Action,
    mode: PermissionMode,
    workspace: &Workspace,
    allowed: &Allowed,
) -> Verdict {
    match action {
        Action::Read { .. }
        | Action::Find { .. }
        | Action::Grep { .. }
        | Action::ShellOutput { .. }
        | Action::ShellStop { .. }
        | Action::WebSearch { .. } => Verdict::Allow,
        Action::WebFetch { url, .. } => fetch_verdict(url, mode, allowed),
        Action::Write { path, .. } | Action::Edit { path, .. } => {
            file_verdict(path, mode, workspace, allowed)
        }
        Action::Shell { command, .. } => shell_verdict(command, mode, allowed),
        Action::Mcp { name, .. } => mcp_verdict(name, mode, allowed),
    }
}

#[cfg(test)]
mod tests;
