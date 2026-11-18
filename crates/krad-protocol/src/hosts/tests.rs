use super::*;
use crate::command::Command;
use crate::event::EventKind;

fn main() -> EnvironmentName {
    EnvironmentName::new("main".to_owned())
}

#[test]
fn a_show_has_its_wire_shape_and_a_newer_verb_reads_as_unknown() -> serde_json::Result<()> {
    let command = Command::Host(HostCommand::Show {
        environment: main(),
    });
    let json = serde_json::to_string(&command)?;
    assert_eq!(
        json,
        r#"{"area":"host","verb":"show","environment":"main"}"#
    );
    assert_eq!(serde_json::from_str::<Command>(&json)?, command);
    let newer: Command = serde_json::from_str(r#"{"area":"host","verb":"forget"}"#)?;
    assert_eq!(newer, Command::Host(HostCommand::Unknown));
    Ok(())
}

#[test]
fn a_sign_in_terminal_is_asked_for_with_its_terminal() -> serde_json::Result<()> {
    let command = Command::Host(HostCommand::OpenSignIn {
        terminal: AgentRef {
            environment: main(),
            terminal: TerminalId(4),
        },
    });
    let json = serde_json::to_string(&command)?;
    assert_eq!(
        json,
        r#"{"area":"host","verb":"open_sign_in","terminal":{"environment":"main","terminal":4}}"#
    );
    assert_eq!(serde_json::from_str::<Command>(&json)?, command);
    Ok(())
}

#[test]
fn an_answer_travels_but_never_prints() -> serde_json::Result<()> {
    let command = HostCommand::AnswerPrompt {
        prompt: PromptId(3),
        answer: Some(PromptAnswer::new("hunter2".to_owned())),
    };
    assert!(!format!("{command:?}").contains("hunter2"));
    let json = serde_json::to_string(&command)?;
    assert_eq!(
        json,
        r#"{"verb":"answer_prompt","prompt":3,"answer":"hunter2"}"#
    );
    assert_eq!(serde_json::from_str::<HostCommand>(&json)?, command);
    Ok(())
}

#[test]
fn host_events_round_trip() -> serde_json::Result<()> {
    let events = [
        EventKind::HostChanged {
            environment: main(),
            state: HostState::Connected {
                platform: HostPlatform::LinuxX86_64,
                home: Some(PathBuf::from("/home/dev")),
                login_env: LoginEnv::Missing,
            },
        },
        EventKind::HostChanged {
            environment: main(),
            state: HostState::Failed {
                reason: "refused".to_owned(),
            },
        },
        EventKind::SignInWanted {
            environment: main(),
        },
        EventKind::MachineChannelWanted {
            environment: main(),
        },
        EventKind::RemoteBusChanged {
            environment: main(),
            bus: RemoteBusState::Ready(RemoteBusInfo {
                remote_port: 41_000,
                bin_dir: PathBuf::from("/home/me/.krad/bin"),
                token_file: PathBuf::from("/home/me/.krad/bus/main.token"),
                endpoint_file: PathBuf::from("/home/me/.krad/bus/main.endpoint"),
                claude_settings: PathBuf::from("/home/me/.krad/claude-hooks.json"),
            }),
        },
        EventKind::HostPrompted(HostPrompt {
            prompt: PromptId(1),
            asker: PromptAsker::Git,
            kind: PromptKind::Secret,
            text: "Password: ".to_owned(),
        }),
        EventKind::HostPromptSettled {
            prompt: PromptId(1),
        },
    ];
    for event in events {
        let json = serde_json::to_string(&event)?;
        assert_eq!(serde_json::from_str::<EventKind>(&json)?, event);
    }
    Ok(())
}

#[test]
fn only_a_connected_host_is_ready_and_only_a_lost_one_reconnects() {
    let connected = HostState::Connected {
        platform: HostPlatform::LinuxAarch64,
        home: None,
        login_env: LoginEnv::Read,
    };
    assert!(connected.is_connected());
    assert!(!connected.can_reconnect());
    assert!(HostState::NotShown.connects_when_shown());
    assert!(!HostState::Disconnected.connects_when_shown());
    assert!(HostState::Reconnecting { attempt: 2 }.can_reconnect());
    assert!(HostState::Reconnecting { attempt: 2 }.is_pending());
    assert_eq!(
        HostState::SigningIn {
            terminal: TerminalId(4)
        }
        .sign_in_terminal(),
        Some(TerminalId(4))
    );
    assert!(RemoteBusState::Preparing.is_preparing());
    assert!(RemoteBusState::Preparing.ready().is_none());
}

#[test]
fn local_agents_are_detected_per_place_and_read_from_older_snapshots() -> serde_json::Result<()> {
    let detect = Command::Host(HostCommand::DetectLocalAgents {
        place: AgentPlace::Wsl {
            distro: "Ubuntu".to_owned(),
        },
    });
    let json = serde_json::to_string(&detect)?;
    assert_eq!(
        json,
        r#"{"area":"host","verb":"detect_local_agents","place":{"place":"wsl","distro":"Ubuntu"}}"#
    );
    assert_eq!(serde_json::from_str::<Command>(&json)?, detect);
    let found = EventKind::LocalAgentsFound(LocalAgents {
        place: AgentPlace::Native,
        agents: vec![AgentPreset::ClaudeCode],
    });
    let json = serde_json::to_string(&found)?;
    assert_eq!(serde_json::from_str::<EventKind>(&json)?, found);
    let older: HostsSnapshot = serde_json::from_str(r#"{"hosts":[],"prompts":[]}"#)?;
    assert_eq!(older.local, Vec::new());
    Ok(())
}

#[test]
fn a_kept_adapter_tells_its_state_and_older_snapshots_have_none() -> serde_json::Result<()> {
    let agent: AcpAgentId = serde_json::from_str(r#""claude-code-acp""#)?;
    let changed = EventKind::AcpAdapterChanged(KeptAdapter {
        agent,
        adapter: ManagedAdapter::Failed("EACCES".to_owned()),
    });
    let json = serde_json::to_string(&changed)?;
    assert_eq!(
        json,
        r#"{"type":"acp_adapter_changed","agent":"claude-code-acp","adapter":{"state":"failed","detail":"EACCES"}}"#
    );
    assert_eq!(serde_json::from_str::<EventKind>(&json)?, changed);
    let retry = Command::Host(HostCommand::RetryAcpAdapter { agent });
    let json = serde_json::to_string(&retry)?;
    assert_eq!(serde_json::from_str::<Command>(&json)?, retry);
    let older: HostsSnapshot = serde_json::from_str(r#"{"hosts":[]}"#)?;
    assert_eq!(older.adapters, Vec::new());
    Ok(())
}
