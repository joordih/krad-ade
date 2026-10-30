use super::*;
use crate::engine::tools::{Occurrence, Replacement, ShellRun};
use std::path::PathBuf;

use PermissionMode::{AcceptEdits, Auto, BypassPermissions, DontAsk, Manual, Plan};
use Verdict::{Allow, Ask, Deny};

const MODES: [PermissionMode; 6] = [Manual, AcceptEdits, Plan, Auto, DontAsk, BypassPermissions];

fn workspace() -> Workspace {
    Workspace {
        cwd: PathBuf::from("/work/app"),
        env: Vec::new(),
    }
}

fn edit(path: &str) -> Action {
    Action::Edit {
        path: PathBuf::from(path),
        replacements: vec![Replacement {
            old: "a".to_owned(),
            new: "b".to_owned(),
            occurrence: Occurrence::Once,
        }],
    }
}

fn shell(command: &str) -> Action {
    Action::Shell {
        command: command.to_owned(),
        timeout: None,
        run: ShellRun::Wait,
    }
}

fn fetch(url: &str) -> Action {
    Action::WebFetch {
        url: url
            .parse()
            .unwrap_or_else(|_| unreachable!("{url} is a web address")),
        prompt: None,
    }
}

fn verdicts(action: &Action, allowed: &Allowed) -> Vec<Verdict> {
    MODES
        .iter()
        .map(|mode| verdict(action, *mode, &workspace(), allowed))
        .collect()
}

#[test]
fn reading_is_always_allowed() {
    let outside = PathBuf::from("/etc");
    let reads = [
        Action::Read {
            path: outside.join("hosts"),
            offset: None,
            limit: None,
        },
        Action::Find {
            pattern: "*.conf".to_owned(),
            path: Some(outside.clone()),
        },
        Action::Grep {
            pattern: "localhost".to_owned(),
            path: Some(outside),
            glob: None,
            ignore_case: false,
        },
        Action::WebSearch {
            query: "rust 2024 edition".to_owned(),
        },
    ];
    for read in reads {
        assert_eq!(verdicts(&read, &Allowed::default()), [Allow; 6], "{read:?}");
    }
}

#[test]
fn edits_follow_the_permission_mode() {
    let allowed = Allowed::default();
    assert_eq!(
        verdicts(&edit("src/lib.rs"), &allowed),
        [
            Ask,
            Allow,
            Deny(PLAN_FILES_TEXT),
            Allow,
            Deny(DONT_ASK_TEXT),
            Allow
        ]
    );
    assert_eq!(
        verdicts(&edit("/etc/hosts"), &allowed),
        [
            Ask,
            Ask,
            Deny(PLAN_FILES_TEXT),
            Ask,
            Deny(DONT_ASK_TEXT),
            Allow
        ]
    );
}

#[test]
fn edits_allowed_always_stop_asking_inside_the_workspace_only() {
    let mut allowed = Allowed::default();
    allowed.remember(&edit("src/lib.rs"));
    assert_eq!(
        verdict(&edit("src/main.rs"), Manual, &workspace(), &allowed),
        Allow
    );
    assert_eq!(
        verdict(&edit("../elsewhere.rs"), Manual, &workspace(), &allowed),
        Ask
    );
    assert_eq!(
        verdict(&edit("src/main.rs"), Plan, &workspace(), &allowed),
        Deny(PLAN_FILES_TEXT)
    );
}

#[test]
fn commands_follow_the_permission_mode() {
    let allowed = Allowed::default();
    assert_eq!(verdicts(&shell("git status"), &allowed), [Allow; 6]);
    assert_eq!(
        verdicts(&shell("cargo test"), &allowed),
        [
            Ask,
            Ask,
            Deny(PLAN_COMMANDS_TEXT),
            Allow,
            Deny(DONT_ASK_TEXT),
            Allow
        ]
    );
    assert_eq!(
        verdicts(&shell("rm -rf target"), &allowed),
        [
            Ask,
            Ask,
            Deny(PLAN_COMMANDS_TEXT),
            Ask,
            Deny(DONT_ASK_TEXT),
            Allow
        ]
    );
}

#[test]
fn a_command_allowed_always_covers_its_subcommand_but_never_a_dangerous_one() {
    let mut allowed = Allowed::default();
    allowed.remember(&shell("cargo test -p krad-chat"));
    assert_eq!(
        verdict(
            &shell("cargo test --workspace"),
            Manual,
            &workspace(),
            &allowed
        ),
        Allow
    );
    assert_eq!(
        verdict(
            &shell("cargo test && git status"),
            Manual,
            &workspace(),
            &allowed
        ),
        Allow
    );
    assert_eq!(
        verdict(&shell("cargo build"), Manual, &workspace(), &allowed),
        Ask
    );
    assert_eq!(
        verdict(&shell("cargo test $(evil)"), Manual, &workspace(), &allowed),
        Ask
    );
    allowed.remember(&shell("rm -rf build"));
    assert_eq!(
        verdict(&shell("rm -rf dist"), AcceptEdits, &workspace(), &allowed),
        Ask
    );
}

#[test]
fn background_shells_start_like_commands_and_are_read_and_stopped_freely() {
    let allowed = Allowed::default();
    let background = Action::Shell {
        command: "npm run dev".to_owned(),
        timeout: None,
        run: ShellRun::Background,
    };
    assert_eq!(
        verdicts(&background, &allowed),
        verdicts(&shell("npm run dev"), &allowed)
    );
    let output = Action::ShellOutput {
        id: "bg1".to_owned(),
    };
    let stop = Action::ShellStop {
        id: "bg1".to_owned(),
    };
    assert_eq!(verdicts(&output, &allowed), [Allow; 6]);
    assert_eq!(verdicts(&stop, &allowed), [Allow; 6]);
}

#[test]
fn fetching_follows_the_mode_and_remembers_hosts() {
    let mut allowed = Allowed::default();
    let page = fetch("https://docs.rs/serde");
    assert_eq!(
        verdicts(&page, &allowed),
        [Ask, Ask, Allow, Allow, Deny(DONT_ASK_HOST_TEXT), Allow]
    );
    allowed.remember(&page);
    assert_eq!(
        verdicts(&fetch("https://docs.rs/tokio"), &allowed),
        [Allow; 6]
    );
    assert_eq!(
        verdict(&fetch("https://crates.io"), Manual, &workspace(), &allowed),
        Ask
    );
}

#[test]
fn local_hosts_are_always_asked_unless_bypassing() {
    let mut allowed = Allowed::default();
    let local = fetch("http://127.0.0.1:8080/admin");
    allowed.remember(&local);
    assert_eq!(
        verdicts(&local, &allowed),
        [Ask, Ask, Ask, Ask, Deny(DONT_ASK_HOST_TEXT), Allow]
    );
}

#[test]
fn mcp_tools_ask_like_commands_and_remember_each_tool() {
    let mcp = |name: &str| Action::Mcp {
        name: name.to_owned(),
        arguments: serde_json::json!({}),
    };
    let mut allowed = Allowed::default();
    let read = mcp("mcp__files__read");
    assert_eq!(
        verdicts(&read, &allowed),
        [
            Ask,
            Ask,
            Deny(PLAN_MCP_TEXT),
            Allow,
            Deny(DONT_ASK_TEXT),
            Allow
        ]
    );
    allowed.remember(&read);
    assert_eq!(
        verdicts(&read, &allowed),
        [
            Allow,
            Allow,
            Deny(PLAN_MCP_TEXT),
            Allow,
            Deny(DONT_ASK_TEXT),
            Allow
        ]
    );
    assert_eq!(
        verdict(&mcp("mcp__files__write"), Manual, &workspace(), &allowed),
        Ask
    );
}
