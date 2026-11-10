mod agents;

use super::request::{file_location, spawned_agent, CliError};
use super::*;
use crate::protocol::{AgentInfo, ApiError, ErrorCode, FilePosition, TerminalState, API_VERSION};
use krad_domain::AgentState;
use std::path::PathBuf;

fn words(text: &[&str]) -> Vec<String> {
    text.iter().map(|word| (*word).to_owned()).collect()
}

fn request_of(verb: &str, args: &[&str]) -> Result<Option<Request>, CliError> {
    let verb = verb.parse::<Verb>()?;
    build_request(verb, &parse_options(&words(args))?)
}

fn agent(terminal: u64, state: TerminalState) -> AgentInfo {
    AgentInfo {
        environment: None,
        terminal,
        title: format!("Codex · {terminal}"),
        group: None,
        state,
        harness: Some(AgentPreset::Codex),
        is_self: false,
    }
}

#[test]
fn spawn_needs_an_agent_and_a_task() -> Result<(), CliError> {
    assert_eq!(
        request_of("spawn", &["--agent", "claude"]),
        Err(CliError::SpawnNeedsTask)
    );
    assert_eq!(
        request_of("spawn", &["--agent", "claude", "fix", "it"])?,
        Some(Request::Spawn {
            agent: AgentPreset::ClaudeCode,
            worktree: None,
            task: "fix it".to_owned(),
        })
    );
    let acp =
        |id: &str| AgentPreset::Acp(id.parse().unwrap_or_else(|_| unreachable!("{id} is an id")));
    for (name, agent) in [
        ("codex", AgentPreset::Codex),
        ("Claude Code", AgentPreset::ClaudeCode),
        ("opencode", AgentPreset::Opencode),
        ("acp:opencode", acp("opencode")),
        ("gemini", acp("gemini")),
        ("codex-acp", acp("codex-acp")),
        ("Claude Code (ACP)", acp("claude-code-acp")),
    ] {
        assert_eq!(spawned_agent(Some(name)), Ok(agent));
    }
    for name in ["Shell", "Gateway", "My Agent", "acp:Bad Id"] {
        assert_eq!(
            spawned_agent(Some(name)),
            Err(CliError::UnknownAgent(name.to_owned()))
        );
    }
    Ok(())
}

#[test]
fn the_usage_names_every_public_verb_under_the_invoked_name() {
    assert_eq!(
        usage(CLI_NAME),
        "Usage: krad <list|agents|status|send|check|wait|read|spawn|close|sessions|session|worktree|open|changes|diff|kraddie|automation|variants|code|tools|tool|api|help> [args] [--json]"
    );
    assert_eq!(program_name(OsStr::new("/usr/local/bin/krad")), CLI_NAME);
    assert_eq!(program_name(OsStr::new("krad.exe")), CLI_NAME);
    assert_eq!(
        program_name(OsStr::new("/opt/krad/krad-ade-agents")),
        CLI_PROGRAM
    );
    assert_eq!(
        "nope".parse::<Verb>(),
        Err(CliError::UnknownCommand("nope".to_owned()))
    );
}

#[test]
fn workspace_verbs_build_their_requests() -> Result<(), CliError> {
    let cases = [
        ("api", vec![], Request::Hello),
        ("status", vec![], Request::List),
        ("sessions", vec!["--json"], Request::Sessions),
        (
            "session",
            vec!["create", "API", "work", "--group", "api"],
            Request::CreateSession {
                name: Some("API work".to_owned()),
                group: Some("api".to_owned()),
            },
        ),
        (
            "session",
            vec!["create"],
            Request::CreateSession {
                name: None,
                group: None,
            },
        ),
        (
            "session",
            vec!["close", "3"],
            Request::CloseSession { session: 3 },
        ),
        (
            "worktree",
            vec!["create", "fix-login"],
            Request::CreateWorktree {
                name: "fix-login".to_owned(),
            },
        ),
        (
            "open",
            vec!["src/main.rs:12:4"],
            Request::OpenFile {
                path: PathBuf::from("src/main.rs"),
                position: Some(FilePosition {
                    line: 12,
                    column: Some(4),
                }),
            },
        ),
        ("changes", vec![], Request::Changes { session: None }),
        ("diff", vec!["2"], Request::Changes { session: Some(2) }),
    ];
    for (verb, args, request) in cases {
        assert_eq!(request_of(verb, &args)?, Some(request), "{verb} {args:?}");
    }
    assert_eq!(
        request_of("session", &[]),
        Err(CliError::SessionNeedsAction)
    );
    assert_eq!(
        request_of("session", &["rename"]),
        Err(CliError::SessionNeedsAction)
    );
    assert_eq!(
        request_of("session", &["close"]),
        Err(CliError::MissingSession)
    );
    assert!(matches!(
        request_of("session", &["close", "x"]),
        Err(CliError::InvalidValue { .. })
    ));
    assert_eq!(
        request_of("worktree", &["create"]),
        Err(CliError::WorktreeNeedsName)
    );
    assert_eq!(request_of("open", &[]), Err(CliError::OpenNeedsFile));
    assert_eq!(
        request_of("session", &["create", "--group"]),
        Err(CliError::FlagNeedsValue("--group".to_owned()))
    );
    Ok(())
}

#[test]
fn file_locations_read_an_optional_line_and_column() {
    let at = |line, column| Some(FilePosition { line, column });
    assert_eq!(file_location("a.rs"), (PathBuf::from("a.rs"), None));
    assert_eq!(
        file_location("a.rs:7"),
        (PathBuf::from("a.rs"), at(7, None))
    );
    assert_eq!(
        file_location("a.rs:7:2"),
        (PathBuf::from("a.rs"), at(7, Some(2)))
    );
    assert_eq!(
        file_location("C:\\src\\a.rs:7"),
        (PathBuf::from("C:\\src\\a.rs"), at(7, None))
    );
    assert_eq!(file_location("notes:x"), (PathBuf::from("notes:x"), None));
}

#[test]
fn answers_map_to_stable_exit_codes() {
    let error = |code| Response::Error(ApiError::new(code, "no"));
    assert_eq!(exit_of(&Response::Agents(Vec::new())), Exit::Ok);
    assert_eq!(exit_of(&Response::TimedOut), Exit::TimedOut);
    assert_eq!(exit_of(&error(ErrorCode::NotFound)), Exit::NotFound);
    assert_eq!(exit_of(&error(ErrorCode::Forbidden)), Exit::Refused);
    assert_eq!(exit_of(&error(ErrorCode::Failed)), Exit::Failed);
    assert_eq!(
        exit_of(&error(ErrorCode::UnsupportedApiVersion)),
        Exit::UnsupportedApi
    );
}

#[test]
fn status_groups_agents_by_state_in_text_and_json() -> Result<(), serde_json::Error> {
    let shell = AgentInfo {
        environment: None,
        harness: None,
        ..agent(1, TerminalState::Running(AgentState::Idle))
    };
    let agents = vec![
        agent(4, TerminalState::Running(AgentState::Working)),
        agent(5, TerminalState::Running(AgentState::WaitingForInput)),
        agent(6, TerminalState::Running(AgentState::Done)),
        shell,
    ];
    let response = Response::Agents(agents);
    let (Output::Stdout(text), Exit::Ok) = report(Verb::Status, &Request::List, &response, false)
    else {
        unreachable!("status prints to stdout");
    };
    assert_eq!(
        text,
        "working (1): 4 Codex · 4\nwaiting-for-input (1): 5 Codex · 5\ndone (1): 6 Codex · 6\nidle (0): none\nexited (0): none"
    );
    let (Output::Stdout(line), _) = report(Verb::Status, &Request::List, &response, true) else {
        unreachable!("status prints to stdout");
    };
    let value: serde_json::Value = serde_json::from_str(&line)?;
    assert_eq!(value["working"][0]["terminal"], 4);
    assert_eq!(value["waiting_for_input"][0]["terminal"], 5);
    assert_eq!(value["done"][0]["terminal"], 6);
    assert_eq!(value["idle"], serde_json::json!([]));
    Ok(())
}

#[test]
fn errors_print_their_code_in_json_and_their_message_in_text() {
    let response = Response::Error(ApiError::new(ErrorCode::NotFound, "No session 9"));
    let request = Request::CloseSession { session: 9 };
    let (Output::Stdout(line), Exit::NotFound) = report(Verb::Session, &request, &response, true)
    else {
        unreachable!("json goes to stdout");
    };
    assert_eq!(
        line,
        r#"{"Error":{"code":"not_found","message":"No session 9"}}"#
    );
    let (Output::Stderr(text), Exit::NotFound) = report(Verb::Session, &request, &response, false)
    else {
        unreachable!("text errors go to stderr");
    };
    assert_eq!(text, "No session 9");
    let hello = Response::Hello(crate::protocol::ApiInfo::current());
    let (Output::Stdout(text), Exit::Ok) = report(Verb::Api, &Request::Hello, &hello, false) else {
        unreachable!("api prints to stdout");
    };
    assert!(text.starts_with(&format!("Agents API {API_VERSION} (build ")));
}

#[test]
fn spawn_starts_an_acp_agent_by_its_id_and_list_names_it() -> Result<(), CliError> {
    let gemini = AgentPreset::Acp(
        "gemini"
            .parse()
            .unwrap_or_else(|_| unreachable!("a valid id")),
    );
    assert_eq!(
        request_of("spawn", &["--agent", "gemini", "write", "tests"])?,
        Some(Request::Spawn {
            agent: gemini,
            worktree: None,
            task: "write tests".to_owned(),
        })
    );
    let listed = AgentInfo {
        environment: None,
        terminal: 5,
        title: "Gemini (ACP) · 5".to_owned(),
        group: None,
        state: TerminalState::Exited,
        harness: Some(gemini),
        is_self: false,
    };
    assert_eq!(
        render::render_agent(&listed),
        "5  Gemini (ACP) · 5  [exited]  agent: acp:gemini"
    );
    Ok(())
}

#[test]
fn close_takes_one_target_and_its_flags() -> Result<(), CliError> {
    use crate::protocol::{CloseOrder, CloseTarget, WhenWorking, WorktreeFate};
    let order = |target, when_working, worktree| {
        Some(Request::Close(CloseOrder {
            target,
            when_working,
            worktree,
        }))
    };
    let cases = [
        (
            vec!["5"],
            order(
                CloseTarget::Agent(5),
                WhenWorking::Refuse,
                WorktreeFate::Keep,
            ),
        ),
        (
            vec!["5", "--force", "--remove-worktree"],
            order(
                CloseTarget::Agent(5),
                WhenWorking::CancelTurn,
                WorktreeFate::Remove,
            ),
        ),
        (
            vec!["--session", "3", "--json"],
            order(
                CloseTarget::Session(3),
                WhenWorking::Refuse,
                WorktreeFate::Keep,
            ),
        ),
        (
            vec!["--all-idle"],
            order(
                CloseTarget::AllIdle,
                WhenWorking::Refuse,
                WorktreeFate::Keep,
            ),
        ),
    ];
    for (args, request) in cases {
        assert_eq!(request_of("close", &args)?, request, "{args:?}");
    }
    for args in [
        vec![],
        vec!["5", "6"],
        vec!["5", "--session", "3"],
        vec!["5", "--all-idle"],
        vec!["--session", "3", "--all-idle"],
    ] {
        assert_eq!(
            request_of("close", &args),
            Err(CliError::CloseNeedsTarget),
            "{args:?}"
        );
    }
    assert!(matches!(
        request_of("close", &["x"]),
        Err(CliError::InvalidValue { .. })
    ));
    assert_eq!(
        request_of("close", &["--session"]),
        Err(CliError::FlagNeedsValue("--session".to_owned()))
    );
    assert_eq!(request_of("list", &["--all"])?, Some(Request::ListAll));
    assert_eq!(request_of("list", &[])?, Some(Request::List));
    Ok(())
}

#[test]
fn kraddie_verbs_list_and_wake_kraddies_by_name() -> Result<(), CliError> {
    assert_eq!(
        request_of("kraddie", &["list", "--json"])?,
        Some(Request::Kraddies)
    );
    assert_eq!(
        request_of("kraddie", &["run", "nightly", "fix", "the", "build"])?,
        Some(Request::RunKraddie {
            name: "nightly".to_owned(),
            prompt: Some("fix the build".to_owned()),
        })
    );
    assert_eq!(
        request_of("kraddie", &["run", "nightly"])?,
        Some(Request::RunKraddie {
            name: "nightly".to_owned(),
            prompt: None,
        })
    );
    assert_eq!(
        request_of("kraddie", &[]),
        Err(CliError::KraddieNeedsAction)
    );
    assert_eq!(
        request_of("kraddie", &["feed"]),
        Err(CliError::KraddieNeedsAction)
    );
    assert_eq!(
        request_of("kraddie", &["run"]),
        Err(CliError::KraddieNeedsName("run"))
    );
    Ok(())
}

#[test]
fn kraddie_chats_memory_and_sharing_go_through_the_cli_and_the_pet_alias() -> Result<(), CliError> {
    assert_eq!(
        request_of("kraddie", &["chat", "new", "nightly"])?,
        Some(Request::NewKraddieChat {
            name: "nightly".to_owned(),
        })
    );
    assert_eq!(
        request_of("pet", &["memory", "show", "nightly", "--json"])?,
        Some(Request::KraddieRecall {
            name: "nightly".to_owned(),
            view: crate::protocol::RecallView::Show,
        })
    );
    assert_eq!(
        request_of("pet", &["share", "nightly", "linter"])?,
        Some(Request::ShareKraddieMemory {
            reader: "nightly".to_owned(),
            from: "linter".to_owned(),
            switch: krad_domain::Switch::On,
        })
    );
    assert_eq!(
        request_of("kraddie", &["unshare", "nightly", "linter"])?,
        Some(Request::ShareKraddieMemory {
            reader: "nightly".to_owned(),
            from: "linter".to_owned(),
            switch: krad_domain::Switch::Off,
        })
    );
    assert_eq!(
        request_of("kraddie", &["chat"]),
        Err(CliError::KraddieChatNeedsNew)
    );
    assert_eq!(
        request_of("kraddie", &["share", "nightly"]),
        Err(CliError::KraddieShareNeedsPair("share"))
    );
    Ok(())
}

#[test]
fn a_refused_share_keeps_its_stable_error_code() {
    let request = Request::ShareKraddieMemory {
        reader: "a".to_owned(),
        from: "a".to_owned(),
        switch: krad_domain::Switch::On,
    };
    let refused = Response::error(
        crate::protocol::ErrorCode::BadRequest,
        "A Kraddie always reads its own memory",
    );
    let (Output::Stdout(text), exit) = report(Verb::Kraddie, &request, &refused, true) else {
        unreachable!("a JSON answer goes to stdout");
    };
    assert!(text.contains("\"bad_request\""), "{text}");
    assert!(!matches!(exit, Exit::Ok));
}

#[test]
fn kraddie_memory_verbs_read_and_add_lines() -> Result<(), CliError> {
    assert_eq!(
        request_of(
            "kraddie",
            &["remember", "nightly", "CI", "uses", "runner", "7"]
        )?,
        Some(Request::RememberKraddie {
            name: "nightly".to_owned(),
            text: "CI uses runner 7".to_owned(),
        })
    );
    assert_eq!(
        request_of("kraddie", &["memory", "nightly", "--json"])?,
        Some(Request::KraddieMemory {
            name: "nightly".to_owned(),
        })
    );
    assert_eq!(
        request_of("kraddie", &["remember", "nightly", "  "]),
        Err(CliError::KraddieNeedsMemory)
    );
    assert_eq!(
        request_of("kraddie", &["remember"]),
        Err(CliError::KraddieNeedsName("remember"))
    );
    assert_eq!(
        request_of("kraddie", &["memory"]),
        Err(CliError::KraddieNeedsName("memory"))
    );
    Ok(())
}

#[test]
fn a_kraddie_memory_prints_numbered_lines_and_its_meter() {
    let memory = Response::KraddieMemory(crate::protocol::KraddieMemoryInfo {
        kraddie: "nightly".to_owned(),
        lines: vec!["CI uses runner 7".to_owned(), "Ship on Fridays".to_owned()],
        limit: 200,
    });
    let request = Request::KraddieMemory {
        name: "nightly".to_owned(),
    };
    let (Output::Stdout(text), Exit::Ok) = report(Verb::Kraddie, &request, &memory, false) else {
        unreachable!("a memory prints to stdout");
    };
    assert_eq!(
        text,
        "1  CI uses runner 7\n2  Ship on Fridays\n2 of 200 kept"
    );
    let empty = Response::KraddieMemory(crate::protocol::KraddieMemoryInfo {
        kraddie: "nightly".to_owned(),
        lines: Vec::new(),
        limit: 200,
    });
    let (Output::Stdout(text), _) = report(Verb::Kraddie, &request, &empty, false) else {
        unreachable!("an empty memory prints to stdout");
    };
    assert_eq!(text, "No keepsakes yet\n0 of 200 kept");
    let full = Response::Error(ApiError::new(ErrorCode::Forbidden, "The memory is full"));
    let remember = Request::RememberKraddie {
        name: "nightly".to_owned(),
        text: "more".to_owned(),
    };
    assert_eq!(
        report(Verb::Kraddie, &remember, &full, false).1,
        Exit::from(ErrorCode::Forbidden)
    );
}

#[test]
fn variants_run_reads_each_variant_with_its_own_model_and_instruction() -> Result<(), CliError> {
    let args = [
        "run",
        "--variant",
        "claude",
        "--model",
        "opus",
        "--variant",
        "codex",
        "--instruction",
        "Keep it small",
        "--test",
        "cargo test",
        "--group",
        "api",
        "Fix",
        "the login",
    ];
    let request = request_of("variants", &args)?;
    let expected = crate::protocol::VariantsRequest {
        prompt: "Fix the login".to_owned(),
        variants: vec![
            krad_domain::VariantSpec::new(
                AgentPreset::ClaudeCode,
                krad_domain::ModelId::parse("opus"),
                None,
            ),
            krad_domain::VariantSpec::new(AgentPreset::Codex, None, Some("Keep it small")),
        ],
        test: Some("cargo test".to_owned()),
        group: Some("api".to_owned()),
    };
    assert_eq!(request, Some(Request::RunVariants(expected)));
    Ok(())
}

#[test]
fn a_close_answer_says_where_each_worktree_is() {
    use crate::protocol::{
        CloseOrder, CloseTarget, Closed, ClosedTarget, WorktreeLeft, WorktreeOutcome,
    };
    let request = Request::Close(CloseOrder {
        target: CloseTarget::AllIdle,
        when_working: crate::protocol::WhenWorking::Refuse,
        worktree: crate::protocol::WorktreeFate::Remove,
    });
    let left = |outcome| {
        Some(WorktreeLeft {
            path: PathBuf::from("/src/app.worktrees/fix"),
            outcome,
        })
    };
    let answer = Response::Closed(vec![
        Closed {
            target: ClosedTarget::Agent(5),
            worktree: left(WorktreeOutcome::Kept),
        },
        Closed {
            target: ClosedTarget::Agent(6),
            worktree: left(WorktreeOutcome::Removed),
        },
        Closed {
            target: ClosedTarget::Session(3),
            worktree: left(WorktreeOutcome::Refused(ApiError::new(
                ErrorCode::WorktreeNotClean,
                "It has uncommitted changes",
            ))),
        },
    ]);
    let (output, exit) = report(Verb::Close, &request, &answer, false);
    assert_eq!(exit, Exit::Ok);
    let Output::Stdout(text) = output else {
        unreachable!("a close answer prints to stdout");
    };
    assert_eq!(
        text,
        "Closed worker 5; its worktree stays at /src/app.worktrees/fix\n\
         Closed worker 6; removed its worktree /src/app.worktrees/fix\n\
         Closed session 3; kept its worktree at /src/app.worktrees/fix (worktree_not_clean): It has uncommitted changes"
    );
    let (nothing, _) = report(Verb::Close, &request, &Response::Closed(Vec::new()), false);
    assert!(matches!(nothing, Output::Stdout(text) if text == "Nothing to close"));
    let refused = Response::error(ErrorCode::NotOwned, "Not yours");
    assert_eq!(
        report(Verb::Close, &request, &refused, false).1,
        Exit::Refused
    );
}

#[test]
fn kraddies_render_as_lines_and_refusals_keep_their_exit_code() {
    let kraddie = crate::protocol::KraddieInfo {
        id: "nightly".to_owned(),
        name: "Nightly".to_owned(),
        switch: krad_domain::Switch::Off,
        harness: "codex".to_owned(),
        schedule: Some("Mon–Fri 06:30".to_owned()),
        next_run: None,
        last_outcome: Some("Finished".to_owned()),
    };
    let list = Response::Kraddies(vec![kraddie]);
    let (Output::Stdout(text), Exit::Ok) = report(Verb::Kraddie, &Request::Kraddies, &list, false)
    else {
        unreachable!("a Kraddie list prints to stdout");
    };
    assert_eq!(
        text,
        "nightly  Nightly  codex  [Mon–Fri 06:30]  last: Finished  (off)"
    );
    let (Output::Stdout(text), Exit::Ok) = report(
        Verb::Kraddie,
        &Request::Kraddies,
        &Response::Kraddies(Vec::new()),
        false,
    ) else {
        unreachable!("an empty list prints to stdout");
    };
    assert_eq!(text, "No Kraddies");
    let run = Request::RunKraddie {
        name: "ghost".to_owned(),
        prompt: None,
    };
    let refused = Response::Error(ApiError::new(ErrorCode::NotFound, "No Kraddie named ghost"));
    let (Output::Stderr(_), Exit::NotFound) = report(Verb::Kraddie, &run, &refused, false) else {
        unreachable!("an unknown Kraddie is not found");
    };
    let busy = Response::Error(ApiError::new(ErrorCode::Unavailable, "busy"));
    let (_, exit) = report(Verb::Kraddie, &run, &busy, false);
    assert_eq!(exit, Exit::from(ErrorCode::Unavailable));
}

#[test]
fn variants_run_refuses_a_missing_task_a_stray_flag_or_too_many_variants() {
    assert_eq!(
        request_of(
            "variants",
            &["run", "--variant", "claude", "--variant", "codex"]
        ),
        Err(CliError::VariantsNeedTask)
    );
    assert_eq!(
        request_of("variants", &["run", "--model", "opus", "fix"]),
        Err(CliError::FlagBeforeVariant("--model".to_owned()))
    );
    assert_eq!(
        request_of("variants", &["run", "--variant", "claude", "fix"]),
        Err(CliError::VariantCount)
    );
    let five: Vec<&str> = std::iter::repeat_n(["--variant", "codex"], 5)
        .flatten()
        .chain(["fix"])
        .collect();
    let mut args = vec!["run"];
    args.extend(five);
    assert_eq!(request_of("variants", &args), Err(CliError::VariantCount));
    assert_eq!(
        request_of(
            "variants",
            &["run", "--variant", "shell", "--variant", "codex", "fix"]
        ),
        Err(CliError::UnknownAgent("shell".to_owned()))
    );
}

#[test]
fn variants_status_and_pick_name_their_run_and_pick_needs_a_yes() -> Result<(), CliError> {
    assert_eq!(
        request_of("variants", &["status"])?,
        Some(Request::Variants { run: None })
    );
    assert_eq!(
        request_of("variants", &["status", "3"])?,
        Some(Request::Variants { run: Some(3) })
    );
    assert_eq!(
        request_of("variants", &["pick", "3", "2"]),
        Err(CliError::PickNeedsConfirmation)
    );
    assert_eq!(
        request_of("variants", &["pick", "3", "2", "--apply", "--yes"])?,
        Some(Request::PickVariant {
            run: 3,
            variant: 2,
            outcome: krad_domain::WinnerOutcome::ApplyOntoBase,
            confirmed: true,
        })
    );
    assert_eq!(
        request_of("variants", &["pick", "3", "--yes"]),
        Err(CliError::PickNeedsVariant)
    );
    assert_eq!(
        request_of("variants", &["stop"]),
        Err(CliError::VariantsNeedAction)
    );
    Ok(())
}

#[test]
fn automation_verbs_list_run_and_switch_automations_by_name() -> Result<(), CliError> {
    assert_eq!(
        request_of("automation", &["list"])?,
        Some(Request::Automations)
    );
    assert_eq!(
        request_of("automation", &["run", "owl/Find bugs", "only", "src"])?,
        Some(Request::RunAutomation {
            name: "owl/Find bugs".to_owned(),
            prompt: Some("only src".to_owned()),
        })
    );
    assert_eq!(
        request_of("automation", &["run", "Docs"])?,
        Some(Request::RunAutomation {
            name: "Docs".to_owned(),
            prompt: None,
        })
    );
    assert_eq!(
        request_of("automation", &["disable", "Docs", "--json"])?,
        Some(Request::SwitchAutomation {
            name: "Docs".to_owned(),
            switch: krad_domain::Switch::Off,
        })
    );
    assert_eq!(
        request_of("automation", &["enable", "Docs"])?,
        Some(Request::SwitchAutomation {
            name: "Docs".to_owned(),
            switch: krad_domain::Switch::On,
        })
    );
    assert_eq!(
        request_of("automation", &[]),
        Err(CliError::AutomationNeedsAction)
    );
    assert_eq!(
        request_of("automation", &["pause", "Docs"]),
        Err(CliError::AutomationNeedsAction)
    );
    assert_eq!(
        request_of("automation", &["enable"]),
        Err(CliError::AutomationNeedsName("enable"))
    );
    assert_eq!(
        request_of("automation", &["run"]),
        Err(CliError::AutomationNeedsName("run"))
    );
    Ok(())
}

#[test]
fn automations_render_as_lines_and_refusals_keep_their_exit_code() {
    let automation = crate::protocol::AutomationInfo {
        kraddie: "owl".to_owned(),
        id: 2,
        name: "Find bugs".to_owned(),
        switch: krad_domain::Switch::Off,
        triggers: vec!["09:00 workdays".to_owned(), "Manual".to_owned()],
        next_run: None,
        last_outcome: Some("Finished".to_owned()),
        runs: 3,
    };
    let list = Response::Automations(vec![automation.clone()]);
    let (Output::Stdout(text), Exit::Ok) =
        report(Verb::Automation, &Request::Automations, &list, false)
    else {
        unreachable!("an automation list prints to stdout");
    };
    assert_eq!(
        text,
        "owl/Find bugs  [09:00 workdays, Manual]  last: Finished  runs: 3  (off)"
    );
    let empty = Response::Automations(Vec::new());
    let (Output::Stdout(text), _) = report(Verb::Automation, &Request::Automations, &empty, false)
    else {
        unreachable!("an empty list prints to stdout");
    };
    assert_eq!(text, "No automations");
    let switch = Request::SwitchAutomation {
        name: "Find bugs".to_owned(),
        switch: krad_domain::Switch::Off,
    };
    let (Output::Stdout(text), Exit::Ok) = report(
        Verb::Automation,
        &switch,
        &Response::AutomationSwitched(Box::new(automation)),
        false,
    ) else {
        unreachable!("a switch prints to stdout");
    };
    assert_eq!(text, "Automation owl/Find bugs is off");
    let run = Request::RunAutomation {
        name: "ghost".to_owned(),
        prompt: None,
    };
    for (code, exit) in [
        (ErrorCode::NotFound, Exit::NotFound),
        (ErrorCode::BadRequest, Exit::Usage),
        (ErrorCode::Unavailable, Exit::from(ErrorCode::Unavailable)),
    ] {
        let refused = Response::Error(ApiError::new(code, "refused"));
        assert_eq!(report(Verb::Automation, &run, &refused, false).1, exit);
    }
}

#[test]
fn the_old_verb_still_reaches_kraddies_but_stays_out_of_the_usage() -> Result<(), CliError> {
    let old = super::verb::LEGACY_KRADDIE_VERB;
    assert_eq!(request_of(old, &["list"])?, Some(Request::Kraddies));
    assert_eq!(
        request_of(old, &["memory", "nightly"])?,
        Some(Request::KraddieMemory {
            name: "nightly".to_owned(),
        })
    );
    assert_eq!(request_of(old, &[]), Err(CliError::KraddieNeedsAction));
    let usage = usage(CLI_NAME);
    assert!(usage.contains("|kraddie|"), "{usage}");
    assert!(!usage.contains(&format!("|{old}|")), "{usage}");
    Ok(())
}
