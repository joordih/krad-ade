use super::*;
use crate::handle::{LeadHandle, TerminalHandle};

#[test]
fn envelope_roundtrips_through_json() -> Result<(), serde_json::Error> {
    let envelope = Envelope {
        api_version: API_VERSION,
        token: BusToken::from("secret"),
        from: Caller::Terminal(TerminalHandle {
            environment: "main".to_owned(),
            terminal: 3,
        }),
        request: Request::Send {
            to: 4,
            body: "done: see PR".to_owned(),
        },
    };
    let line = serde_json::to_string(&envelope)?;
    assert!(!line.contains('\n'));
    assert_eq!(serde_json::from_str::<Envelope>(&line)?, envelope);
    Ok(())
}

#[test]
fn only_a_lead_may_start_or_direct_workers() {
    let spawn = Request::Spawn {
        agent: AgentPreset::Codex,
        worktree: None,
        task: "tests".to_owned(),
    };
    let send = Request::Send {
        to: 4,
        body: "also cover clock skew".to_owned(),
    };
    let worker = Caller::Terminal(TerminalHandle {
        environment: "main".to_owned(),
        terminal: 4,
    });
    let lead = Caller::Lead(LeadHandle {
        environment: "main".to_owned(),
        session: 2,
    });
    for request in [&spawn, &send] {
        assert!(matches!(
            lead_refusal(&worker, request),
            Some(Response::Error(ApiError {
                code: ErrorCode::Forbidden,
                ..
            }))
        ));
        assert_eq!(lead_refusal(&lead, request), None);
    }
    assert_eq!(lead_refusal(&worker, &Request::List), None);
}

#[test]
fn harnesses_and_states_keep_their_strings_on_the_wire() -> Result<(), serde_json::Error> {
    let hook = Request::Hook {
        harness: AgentPreset::ClaudeCode,
        event: "Stop".to_owned(),
        tool: None,
        detail: None,
        resume: None,
        mode: None,
    };
    let hook_line = r#"{"Hook":{"harness":"claude","event":"Stop","tool":null,"detail":null}}"#;
    assert_eq!(serde_json::to_string(&hook)?, hook_line);
    assert_eq!(serde_json::from_str::<Request>(hook_line)?, hook);
    let spawn = Request::Spawn {
        agent: AgentPreset::Opencode,
        worktree: None,
        task: "tests".to_owned(),
    };
    let spawn_line = r#"{"Spawn":{"agent":"opencode","worktree":null,"task":"tests"}}"#;
    assert_eq!(serde_json::to_string(&spawn)?, spawn_line);
    assert_eq!(serde_json::from_str::<Request>(spawn_line)?, spawn);
    let codex = r#"{"Spawn":{"agent":"codex","worktree":null,"task":"tests"}}"#;
    assert!(matches!(
        serde_json::from_str::<Request>(codex)?,
        Request::Spawn {
            agent: AgentPreset::Codex,
            ..
        }
    ));
    assert!(serde_json::from_str::<Request>(&spawn_line.replace("opencode", "zsh")).is_err());
    let agent = AgentInfo {
        environment: None,
        terminal: 4,
        title: "Claude Code · 4".to_owned(),
        group: None,
        state: TerminalState::Running(AgentState::WaitingForInput),
        harness: Some(AgentPreset::ClaudeCode),
        is_self: false,
    };
    let agent_line = r#"{"terminal":4,"title":"Claude Code · 4","group":null,"state":"waiting-for-input","harness":"Claude Code","is_self":false}"#;
    assert_eq!(serde_json::to_string(&agent)?, agent_line);
    assert_eq!(serde_json::from_str::<AgentInfo>(agent_line)?, agent);
    let unknown = AgentInfo {
        harness: None,
        ..agent
    };
    assert_eq!(
        serde_json::to_string(&unknown)?,
        agent_line.replace(r#""Claude Code","is_self""#, r#"null,"is_self""#)
    );
    let settled = Response::Settled {
        target: 4,
        state: TerminalState::Exited,
    };
    let settled_line = r#"{"Settled":{"target":4,"state":"exited"}}"#;
    assert_eq!(serde_json::to_string(&settled)?, settled_line);
    assert_eq!(serde_json::from_str::<Response>(settled_line)?, settled);
    for state in AgentState::ALL {
        let line = serde_json::to_string(&TerminalState::Running(state))?;
        assert_eq!(line, format!("\"{}\"", state.label()));
    }
    Ok(())
}

#[test]
fn acp_agents_travel_by_their_id() -> Result<(), serde_json::Error> {
    let gemini = AgentPreset::Acp(
        "gemini"
            .parse()
            .unwrap_or_else(|_| unreachable!("a valid id")),
    );
    let spawn = Request::Spawn {
        agent: gemini,
        worktree: None,
        task: "tests".to_owned(),
    };
    let spawn_line = r#"{"Spawn":{"agent":"acp:gemini","worktree":null,"task":"tests"}}"#;
    assert_eq!(serde_json::to_string(&spawn)?, spawn_line);
    assert_eq!(serde_json::from_str::<Request>(spawn_line)?, spawn);
    let agent = AgentInfo {
        environment: None,
        terminal: 5,
        title: "Gemini (ACP) · 5".to_owned(),
        group: None,
        state: TerminalState::Running(AgentState::Working),
        harness: Some(gemini),
        is_self: false,
    };
    let agent_line = r#"{"terminal":5,"title":"Gemini (ACP) · 5","group":null,"state":"working","harness":"acp:gemini","is_self":false}"#;
    assert_eq!(serde_json::to_string(&agent)?, agent_line);
    assert_eq!(serde_json::from_str::<AgentInfo>(agent_line)?, agent);
    let unknown = agent_line.replace("acp:gemini", "acp:Not An Id");
    assert!(serde_json::from_str::<AgentInfo>(&unknown).is_err());
    Ok(())
}

#[test]
fn extension_tool_requests_and_answers_roundtrip() -> Result<(), serde_json::Error> {
    use crate::extension_tools::{ExtensionToolEffect, ExtensionToolInfo, ExtensionToolOutcome};
    let call = Request::ExtensionTool {
        name: "ext_docker_restart".to_owned(),
        arguments: serde_json::json!({"id": "web"}),
    };
    let line = serde_json::to_string(&call)?;
    assert_eq!(serde_json::from_str::<Request>(&line)?, call);
    assert_eq!(serde_json::to_string(&Request::Tools)?, r#""Tools""#);
    let catalog = Response::Tools(ToolCatalog {
        code_tools: krad_domain::CodeTools::Off,
        extension_tools: vec![ExtensionToolInfo {
            name: "ext_docker_restart".to_owned(),
            extension: "Docker".to_owned(),
            description: "Restart a container".to_owned(),
            input_schema: serde_json::json!({"type": "object"}),
            effect: ExtensionToolEffect::Destructive,
        }],
        memory_tools: crate::protocol::MemoryTools::On,
    });
    let line = serde_json::to_string(&catalog)?;
    assert_eq!(serde_json::from_str::<Response>(&line)?, catalog);
    let answer = Response::ExtensionTool(ExtensionToolAnswer {
        text: "restarted".to_owned(),
        outcome: ExtensionToolOutcome::Done,
    });
    let line = serde_json::to_string(&answer)?;
    assert_eq!(serde_json::from_str::<Response>(&line)?, answer);
    assert!(call.deadline() > Request::Tools.deadline());
    Ok(())
}

#[test]
fn debug_hides_the_token() {
    let envelope = Envelope {
        api_version: API_VERSION,
        token: BusToken::from("secret"),
        from: Caller::Terminal(TerminalHandle {
            environment: "main".to_owned(),
            terminal: 3,
        }),
        request: Request::Send {
            to: 4,
            body: "hi".to_owned(),
        },
    };
    let printed = format!("{envelope:?}");
    assert!(!printed.contains("secret"));
    assert!(printed.contains("[REDACTED]"));
}

#[test]
fn an_envelope_without_a_version_speaks_the_first_api() -> Result<(), serde_json::Error> {
    let line = r#"{"token":"secret","from":{"Terminal":{"environment":"main","terminal":3}},"request":"List"}"#;
    let envelope = serde_json::from_str::<Envelope>(line)?;
    assert_eq!(envelope.api_version, ApiVersion(1));
    let versioned = serde_json::to_string(&envelope)?;
    assert!(versioned.starts_with(r#"{"api_version":1,"#), "{versioned}");
    Ok(())
}

#[test]
fn workspace_requests_keep_their_shape_on_the_wire() -> Result<(), serde_json::Error> {
    let cases = [
        (Request::Hello, r#""Hello""#),
        (Request::Sessions, r#""Sessions""#),
        (
            Request::CreateSession {
                name: Some("API".to_owned()),
                group: None,
            },
            r#"{"CreateSession":{"name":"API","group":null}}"#,
        ),
        (
            Request::CloseSession { session: 3 },
            r#"{"CloseSession":{"session":3}}"#,
        ),
        (
            Request::CreateWorktree {
                name: "fix-login".to_owned(),
            },
            r#"{"CreateWorktree":{"name":"fix-login"}}"#,
        ),
        (
            Request::OpenFile {
                path: PathBuf::from("src/main.rs"),
                position: Some(FilePosition {
                    line: 12,
                    column: None,
                }),
            },
            r#"{"OpenFile":{"path":"src/main.rs","position":{"line":12}}}"#,
        ),
        (
            Request::Changes { session: None },
            r#"{"Changes":{"session":null}}"#,
        ),
    ];
    for (request, line) in cases {
        assert_eq!(serde_json::to_string(&request)?, line);
        assert_eq!(serde_json::from_str::<Request>(line)?, request);
    }
    assert!(Request::Changes { session: None }.deadline() > Request::Sessions.deadline());
    Ok(())
}

#[test]
fn answers_carry_a_stable_error_code() -> Result<(), serde_json::Error> {
    let refused = Response::error(ErrorCode::Forbidden, "no");
    let line = r#"{"Error":{"code":"forbidden","message":"no"}}"#;
    assert_eq!(serde_json::to_string(&refused)?, line);
    assert_eq!(serde_json::from_str::<Response>(line)?, refused);
    let changes = Response::Changes(ChangeSet {
        group: "api".to_owned(),
        root: PathBuf::from("/repo"),
        branch: Some("main".to_owned()),
        files: vec![ChangedFile {
            path: PathBuf::from("a.rs"),
            kind: krad_domain::ChangeKind::Modified,
        }],
        patch: String::new(),
    });
    let line = serde_json::to_string(&changes)?;
    assert!(line.contains(r#""kind":"modified""#), "{line}");
    assert_eq!(serde_json::from_str::<Response>(&line)?, changes);
    Ok(())
}

#[test]
fn kraddie_requests_and_answers_keep_their_shape_on_the_wire() -> Result<(), serde_json::Error> {
    let cases = [
        (Request::Kraddies, r#""Kraddies""#),
        (
            Request::RunKraddie {
                name: "nightly".to_owned(),
                prompt: Some("go".to_owned()),
            },
            r#"{"RunKraddie":{"name":"nightly","prompt":"go"}}"#,
        ),
    ];
    for (request, line) in cases {
        assert_eq!(serde_json::to_string(&request)?, line);
        assert_eq!(serde_json::from_str::<Request>(line)?, request);
    }
    let started = Response::KraddieStarted(KraddieRunInfo {
        kraddie: "nightly".to_owned(),
        run: 4,
    });
    let line = r#"{"KraddieStarted":{"kraddie":"nightly","run":4}}"#;
    assert_eq!(serde_json::to_string(&started)?, line);
    assert_eq!(serde_json::from_str::<Response>(line)?, started);
    let listed = Response::Kraddies(vec![KraddieInfo {
        id: "nightly".to_owned(),
        name: "Nightly".to_owned(),
        switch: krad_domain::Switch::On,
        harness: "claude".to_owned(),
        schedule: None,
        next_run: None,
        last_outcome: None,
    }]);
    let line =
        r#"{"Kraddies":[{"id":"nightly","name":"Nightly","switch":"on","harness":"claude"}]}"#;
    assert_eq!(serde_json::to_string(&listed)?, line);
    assert_eq!(serde_json::from_str::<Response>(line)?, listed);
    let remember = Request::RememberKraddie {
        name: "nightly".to_owned(),
        text: "CI uses runner 7".to_owned(),
    };
    let line = r#"{"RememberKraddie":{"name":"nightly","text":"CI uses runner 7"}}"#;
    assert_eq!(serde_json::to_string(&remember)?, line);
    assert_eq!(serde_json::from_str::<Request>(line)?, remember);
    let memory = Response::KraddieMemory(super::KraddieMemoryInfo {
        kraddie: "nightly".to_owned(),
        lines: vec!["CI uses runner 7".to_owned()],
        limit: 200,
    });
    let line =
        r#"{"KraddieMemory":{"kraddie":"nightly","lines":["CI uses runner 7"],"limit":200}}"#;
    assert_eq!(serde_json::to_string(&memory)?, line);
    assert_eq!(serde_json::from_str::<Response>(line)?, memory);
    Ok(())
}

#[test]
fn variant_requests_keep_their_shape_on_the_wire() -> Result<(), serde_json::Error> {
    let run = Request::RunVariants(VariantsRequest {
        prompt: "Fix it".to_owned(),
        variants: vec![krad_domain::VariantSpec::new(
            AgentPreset::Codex,
            krad_domain::ModelId::parse("gpt-5.5"),
            None,
        )],
        test: None,
        group: None,
    });
    let line = serde_json::to_string(&run)?;
    assert_eq!(
        line,
        r#"{"RunVariants":{"prompt":"Fix it","variants":[{"harness":"Codex","model":"gpt-5.5"}]}}"#
    );
    assert_eq!(serde_json::from_str::<Request>(&line)?, run);
    let pick = Request::PickVariant {
        run: 4,
        variant: 1,
        outcome: krad_domain::WinnerOutcome::KeepWorktree,
        confirmed: true,
    };
    let line =
        r#"{"PickVariant":{"run":4,"variant":1,"outcome":"keep_worktree","confirmed":true}}"#;
    assert_eq!(serde_json::to_string(&pick)?, line);
    assert!(run.deadline() > Request::Variants { run: None }.deadline());
    Ok(())
}

#[test]
fn automation_requests_and_answers_keep_their_shape_on_the_wire() -> Result<(), serde_json::Error> {
    let cases = [
        (Request::Automations, r#""Automations""#),
        (
            Request::RunAutomation {
                name: "owl/Docs".to_owned(),
                prompt: None,
            },
            r#"{"RunAutomation":{"name":"owl/Docs","prompt":null}}"#,
        ),
        (
            Request::SwitchAutomation {
                name: "Docs".to_owned(),
                switch: krad_domain::Switch::On,
            },
            r#"{"SwitchAutomation":{"name":"Docs","switch":"on"}}"#,
        ),
    ];
    for (request, line) in cases {
        assert_eq!(serde_json::to_string(&request)?, line);
        assert_eq!(serde_json::from_str::<Request>(line)?, request);
        assert_eq!(request.deadline(), DEFAULT_DEADLINE);
    }
    let started = Response::AutomationStarted(AutomationRunInfo {
        kraddie: "owl".to_owned(),
        automation: "Docs".to_owned(),
        run: 7,
    });
    let line = r#"{"AutomationStarted":{"kraddie":"owl","automation":"Docs","run":7}}"#;
    assert_eq!(serde_json::to_string(&started)?, line);
    assert_eq!(serde_json::from_str::<Response>(line)?, started);
    let listed = r#"{"Automations":[{"kraddie":"owl","id":1,"name":"Docs","switch":"off"}]}"#;
    let Response::Automations(read) = serde_json::from_str::<Response>(listed)? else {
        unreachable!("a list of automations");
    };
    assert_eq!(read.first().map(|info| info.runs), Some(0));
    Ok(())
}
