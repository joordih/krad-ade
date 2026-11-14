use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{AgentPreset, Host, ModelOffer};
use krad_protocol::{
    Command, EventKind, Feed, ListedModels, ModelsCommand, ModelsOrigin, Reply, Scope,
};
use krad_runtime::{EventReceiver, Runtime, RuntimeHandle, RuntimeSetup};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

const FEED_ENDED: &str = "the feed ended";

fn subscribed(runtime: &RuntimeHandle) -> Result<EventReceiver, Box<dyn Error>> {
    let mut events = runtime.subscribe();
    match block_on(events.next()).ok_or(FEED_ENDED)? {
        Feed::Snapshot(_) => Ok(events),
        Feed::Event(_) => Err("the feed did not start from a snapshot".into()),
    }
}

fn next_model_event(events: &mut EventReceiver) -> Result<EventKind, Box<dyn Error>> {
    loop {
        if let Feed::Event(event) = block_on(events.next()).ok_or(FEED_ENDED)? {
            if matches!(
                event.kind,
                EventKind::ModelsListed { .. } | EventKind::ModelListFailed { .. }
            ) {
                return Ok(event.kind);
            }
        }
    }
}

fn offer(value: &str) -> ModelOffer {
    ModelOffer {
        value: value.to_owned(),
        name: value.to_owned(),
        ..ModelOffer::default()
    }
}

fn keep(runtime: &RuntimeHandle, harness: AgentPreset, models: Vec<ModelOffer>) -> TestResult {
    let reply = block_on(runtime.send(
        runtime.next_request(),
        Command::Models(ModelsCommand::Keep {
            host: Host::Local,
            harness,
            models,
        }),
    ))?;
    assert_eq!(reply, Reply::Accepted);
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_listing_runs_once_per_host_and_harness_and_reports_its_failure() -> TestResult {
    use krad_domain::Settings;
    use krad_protocol::HarnessSpot;
    use krad_runtime::SettingsSetup;
    use std::collections::HashMap;
    use std::path::Path;

    let codex = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-harness/codex");
    let runtime = Runtime::start(RuntimeSetup {
        settings: SettingsSetup {
            settings: Settings {
                harness_commands: HashMap::from([(
                    AgentPreset::Codex,
                    codex.display().to_string(),
                )]),
                ..Settings::default()
            },
            file: None,
        },
        ..RuntimeSetup::default()
    })?;
    let mut events = subscribed(&runtime)?;
    let list = || -> TestResult {
        let reply = block_on(runtime.send(
            runtime.next_request(),
            Command::Models(ModelsCommand::List {
                spot: HarnessSpot::default(),
                harness: AgentPreset::Codex,
            }),
        ))?;
        assert_eq!(reply, Reply::Accepted);
        Ok(())
    };
    list()?;
    list()?;
    assert!(matches!(
        next_model_event(&mut events)?,
        EventKind::ModelListFailed {
            harness: AgentPreset::Codex,
            ..
        }
    ));
    keep(&runtime, AgentPreset::Codex, vec![offer("gpt-5")])?;
    assert_eq!(
        next_model_event(&mut events)?,
        EventKind::ModelsListed {
            listed: ListedModels {
                host: Host::Local,
                harness: AgentPreset::Codex,
                models: vec![offer("gpt-5")],
            },
            origin: ModelsOrigin::Chat,
        },
        "the second listing was never started, so the next event is the kept list"
    );
    Ok(())
}

#[test]
fn a_chat_keeps_the_models_of_its_harness_on_its_host_unless_none_are_offered() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let mut events = subscribed(&runtime)?;
    keep(&runtime, AgentPreset::ClaudeCode, Vec::new())?;
    keep(&runtime, AgentPreset::Shell, vec![offer("big-pickle")])?;
    keep(&runtime, AgentPreset::ClaudeCode, vec![offer("opus")])?;
    let EventKind::ModelsListed { listed, origin } = next_model_event(&mut events)? else {
        return Err("a model list was expected".into());
    };
    assert_eq!(
        (listed.harness, origin),
        (AgentPreset::ClaudeCode, ModelsOrigin::Chat)
    );
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert_eq!(snapshot.models.listed, vec![listed]);
    Ok(())
}

#[test]
fn an_acp_agent_off_path_reports_its_listing_as_failed() -> TestResult {
    use krad_domain::{AcpAgent, AcpSettings, Settings};
    use krad_protocol::{AcpListing, AcpOffers, HarnessSpot};
    use krad_runtime::SettingsSetup;

    let agent = AcpAgent {
        id: "krad-test-missing".parse()?,
        name: "Missing".to_owned(),
        program: "krad-test-no-such-acp-agent".to_owned(),
        args: Vec::new(),
        env: Vec::new(),
        terminal: None,
    };
    let id = agent.id;
    let runtime = Runtime::start(RuntimeSetup {
        settings: SettingsSetup {
            settings: Settings {
                acp: AcpSettings {
                    agents: vec![agent],
                },
                ..Settings::default()
            },
            file: None,
        },
        ..RuntimeSetup::default()
    })?;
    let mut events = subscribed(&runtime)?;
    let reply = block_on(runtime.send(
        runtime.next_request(),
        Command::Models(ModelsCommand::List {
            spot: HarnessSpot::default(),
            harness: AgentPreset::Acp(id),
        }),
    ))?;
    assert_eq!(reply, Reply::Accepted);
    let offers = loop {
        if let Feed::Event(event) = block_on(events.next()).ok_or(FEED_ENDED)? {
            if let EventKind::AcpOffers(offers) = event.kind {
                break offers;
            }
        }
    };
    assert!(matches!(
        offers,
        AcpOffers {
            host: Host::Local,
            agent,
            listing: AcpListing::Failed { ref error },
        } if agent == id && error.ends_with("is not on PATH")
    ));
    Ok(())
}
