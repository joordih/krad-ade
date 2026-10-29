use super::*;
use krad_domain::{Effort, PermissionMode};

fn model(name: &str) -> Option<ModelId> {
    ModelId::parse(name)
}

#[test]
fn claude_streams_json_with_the_picked_model() {
    let command = chat_command(AgentPreset::ClaudeCode, model("opus").as_ref(), &[], None);
    assert_eq!(
        command.map(|command| command.args),
        Some(
            [
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--model",
                "opus"
            ]
            .map(str::to_owned)
            .to_vec()
        )
    );
}

#[test]
fn codex_reads_the_prompt_from_stdin_last() {
    let command = chat_command(
        AgentPreset::Codex,
        model("gpt-5").as_ref(),
        &[PathBuf::from("/tmp")],
        None,
    );
    let args = command.map_or_default(|command| command.args);
    assert_eq!(args.last().map(String::as_str), Some("-"));
    assert!(args.windows(2).any(|pair| pair == ["-m", "gpt-5"]));
}

#[test]
fn claude_and_gemini_are_let_into_the_extra_folders() {
    let dirs = [PathBuf::from("/notes")];
    for (preset, flag) in [
        (AgentPreset::ClaudeCode, CLAUDE_ADD_DIR),
        (AgentPreset::Gemini, GEMINI_ADD_DIR),
    ] {
        let args = chat_command(preset, None, &dirs, None).map_or_default(|command| command.args);
        assert!(
            args.windows(2).any(|pair| pair == [flag, "/notes"]),
            "{preset:?}"
        );
    }
}

#[test]
fn an_orchestrating_claude_may_run_the_bus_cli_and_wait_long() {
    let command = chat_command(AgentPreset::ClaudeCode, None, &[], Some("krad-ade-agents"));
    let Some(command) = command else {
        unreachable!("claude has a chat command");
    };
    assert!(command
        .args
        .windows(2)
        .any(|pair| pair == [CLAUDE_ALLOWED_TOOLS, "Bash(krad-ade-agents:*)"]));
    assert!(command
        .env
        .iter()
        .any(|(name, value)| name == CLAUDE_BASH_MAX_TIMEOUT && value == "1860000"));
    let codex = chat_command(AgentPreset::Codex, None, &[], Some("krad-ade-agents"));
    assert!(codex.is_some_and(|command| command.env.is_empty()
        && !command.args.contains(&CLAUDE_ALLOWED_TOOLS.to_owned())));
}

#[test]
fn orchestration_debug_hides_the_bus_token() {
    let orchestration = Orchestration {
        cli: "krad-ade-agents".to_owned(),
        env: BusVars(vec![("KRAD_ADE_TOKEN".to_owned(), "secret".to_owned())]),
    };
    let printed = format!("{orchestration:?}");
    assert!(printed.contains("KRAD_ADE_TOKEN"));
    assert!(!printed.contains("secret"));
}

fn live_config() -> LiveConfig {
    LiveConfig {
        runner: crate::Runner::Native(PathBuf::from("claude")),
        model: None,
        cwd: PathBuf::from("/work"),
        extra_dirs: Vec::new(),
        orchestration: None,
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars::default(),
        approvals: Approvals::Refused,
    }
}

fn session_id() -> ResumeId {
    "0b7c3a52-9f1e-4d2a-8c6b-1f2e3d4c5b6a"
        .parse()
        .unwrap_or_else(|_| unreachable!("a uuid is a valid resume id"))
}

#[test]
fn a_live_claude_reads_stream_json_and_starts_or_resumes_its_session() {
    let id = session_id();
    let started = live_chat_command(
        &LiveConfig {
            model: model("opus"),
            ..live_config()
        },
        &LiveSession::Start(id.clone()),
    );
    assert_eq!(
        started.args,
        [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            "opus",
            CLAUDE_SESSION_ID,
            id.as_str(),
        ]
        .map(str::to_owned)
    );
    let resumed = live_chat_command(
        &LiveConfig {
            extra_dirs: vec![PathBuf::from("/notes")],
            orchestration: Some(Orchestration {
                cli: "krad-ade-agents".to_owned(),
                env: BusVars::default(),
            }),
            ..live_config()
        },
        &LiveSession::Resume(id.clone()),
    );
    assert!(resumed
        .args
        .ends_with(&["--resume".to_owned(), id.as_str().to_owned()]));
    assert!(resumed
        .args
        .windows(2)
        .any(|pair| pair == [CLAUDE_ADD_DIR, "/notes"]));
    assert!(!resumed.env.is_empty(), "an orchestrator waits long");
    let forked = live_chat_command(&live_config(), &LiveSession::Fork(id.clone()));
    assert!(forked
        .args
        .ends_with(&["--resume", id.as_str(), "--fork-session"].map(str::to_owned)));
    assert!(AgentPreset::all()
        .into_iter()
        .all(|preset| !supports_live_chat(preset) || supports_chat(preset)));
}

#[test]
fn a_live_claude_asking_approvals_takes_its_prompt_and_effort_last() {
    let command = live_chat_command(
        &LiveConfig {
            approvals: Approvals::Asked,
            system_prompt: Some("Use the bus.".to_owned()),
            effort: Some(Effort::ExtraHigh),
            ..live_config()
        },
        &LiveSession::Start(session_id()),
    );
    assert!(command.args.ends_with(
        &[
            "--permission-prompt-tool",
            "stdio",
            "--append-system-prompt",
            "Use the bus.",
            "--effort",
            "xhigh",
        ]
        .map(str::to_owned)
    ));
    let refused = live_chat_command(&live_config(), &LiveSession::Start(session_id()));
    assert!(!refused
        .args
        .contains(&CLAUDE_PERMISSION_PROMPT_TOOL.to_owned()));
}

#[test]
fn a_live_claude_starts_in_its_permission_mode_or_in_its_own_setting() {
    let picked = live_chat_command(
        &LiveConfig {
            permission_mode: Some(PermissionMode::AcceptEdits),
            ..live_config()
        },
        &LiveSession::Resume(session_id()),
    );
    assert!(picked
        .args
        .windows(2)
        .any(|pair| pair == [CLAUDE_PERMISSION_MODE, "acceptEdits"]));
    let own = live_chat_command(&live_config(), &LiveSession::Start(session_id()));
    assert!(!own.args.contains(&CLAUDE_PERMISSION_MODE.to_owned()));
}

#[test]
fn only_harnesses_with_a_json_mode_chat() {
    for preset in AgentPreset::all() {
        assert_eq!(
            chat_command(preset, None, &[], None).is_some(),
            supports_chat(preset)
        );
    }
    assert!(!supports_chat(AgentPreset::Shell));
}

#[test]
fn a_resumed_or_forked_chat_opens_with_the_original_history() {
    let id = session_id();
    assert_eq!(LiveSession::Start(id.clone()).history(), None);
    assert_eq!(LiveSession::Resume(id.clone()).history(), Some(&id));
    assert_eq!(
        LiveSession::Fork(id.clone()).history(),
        Some(&id),
        "a copy opens with the original's history"
    );
}
