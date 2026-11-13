use super::*;
use krad_domain::{
    agent_event, hook_state, AgentPreset, AgentState, AppEvent, CloudProcess, EnvironmentName,
    JournalOffset, TerminalId, TerminalNode,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn node(id: u64, process: Option<&str>) -> Result<TerminalNode, Box<dyn std::error::Error>> {
    let mut node: TerminalNode = serde_json::from_value(
        serde_json::json!({"id": id, "title": "Claude", "state": "Working"}),
    )?;
    node.harness = Some(AgentPreset::ClaudeCode);
    node.cloud = match process {
        Some(process) => Some(CloudProcess {
            id: process.parse()?,
            offset: JournalOffset::default(),
        }),
        None => None,
    };
    Ok(node)
}

fn environment(name: &str, nodes: Vec<TerminalNode>) -> EnvironmentData {
    let mut data = EnvironmentData::empty(EnvironmentName::new(name.to_owned()));
    data.nodes = nodes;
    data
}

#[test]
fn a_queued_event_finds_its_cloud_node_by_process_id() -> TestResult {
    let api = environment("api", vec![node(1, None)?, node(2, Some("api-2-a1b2c3"))?]);
    let web = environment("web", vec![node(2, Some("web-2-d4e5f6"))?]);
    let environments = [api, web];
    let found = |process: &str| -> Result<Option<AgentRef>, Box<dyn std::error::Error>> {
        Ok(node_of_process(&environments, &process.parse()?))
    };
    let at = |name: &str, id: u64| AgentRef {
        environment: EnvironmentName::new(name.to_owned()),
        terminal: TerminalId(id),
    };
    assert_eq!(found("web-2-d4e5f6")?, Some(at("web", 2)));
    assert_eq!(found("api-2-a1b2c3")?, Some(at("api", 2)));
    assert_eq!(found("other-9-ffffff")?, None);
    Ok(())
}

#[test]
fn queued_hooks_become_agent_events_and_asks_raise_attention() -> TestResult {
    let text = [
        r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:00.000Z","request":{"Hook":{"harness":"claude","event":"Notification","tool":"permission_prompt","detail":"Claude needs your permission to use Bash"}}}"#,
        r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:01.000Z","request":{"Ask":{"tool":"Bash","input":null,"suggestions":[],"call":null}}}"#,
        r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:02.000Z","request":{"Ask":{"tool":"AskUserQuestion","input":null,"suggestions":[],"call":null}}}"#,
        r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:03.000Z","request":"List"}"#,
    ]
    .join("\n");
    let replays: Vec<Replay> = queued_events(&text)
        .into_iter()
        .filter_map(|event| replay_of(event.request))
        .collect();
    assert_eq!(replays.len(), 3);
    assert_eq!(replays[1], Replay::Attention(AttentionKind::Permission));
    assert_eq!(replays[2], Replay::Attention(AttentionKind::Ask));
    let Replay::Hook(report) = &replays[0] else {
        return Err("the hook was not replayed".into());
    };
    let state = hook_state(report.harness, &report.event, report.tool.as_deref())
        .ok_or("the hook carries no state")?;
    assert_eq!(
        agent_event(
            TerminalId(2),
            Some(report.harness),
            AgentState::Working,
            state
        ),
        Some(AppEvent::AgentWaiting {
            terminal: TerminalId(2)
        })
    );
    Ok(())
}

fn app_events(node: &TerminalNode, events: &[QueuedEvent]) -> Vec<AppEvent> {
    let mut state = node.state;
    events
        .iter()
        .filter_map(|queued| {
            let Request::Hook {
                harness,
                event,
                tool,
                ..
            } = &queued.request
            else {
                return None;
            };
            let after = hook_state(*harness, event, tool.as_deref())?;
            let before = std::mem::replace(&mut state, after);
            (before != after)
                .then(|| agent_event(node.id, node.harness, before, after))
                .flatten()
        })
        .collect()
}

#[test]
fn the_runner_maps_queued_hooks_to_the_events_the_app_publishes() -> TestResult {
    let env = EnvironmentName::new("api".to_owned());
    let lines = [
        r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:00.000Z","request":{"Hook":{"harness":"claude","event":"Stop"}}}"#,
        r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:01.000Z","request":{"Hook":{"harness":"claude","event":"Stop"}}}"#,
        r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:02.000Z","request":{"Hook":{"harness":"claude","event":"UserPromptSubmit"}}}"#,
        r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:03.000Z","request":{"Hook":{"harness":"claude","event":"PreToolUse","tool":"Bash"}}}"#,
        r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:04.000Z","request":{"Hook":{"harness":"claude","event":"Notification","tool":"permission_prompt"}}}"#,
        r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:05.000Z","request":{"Ask":{"tool":"Bash","input":null,"suggestions":[],"call":null}}}"#,
        r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:06.000Z","request":{"Hook":{"harness":"claude","event":"Stop"}}}"#,
    ]
    .join("\n");
    let events = queued_events(&lines);
    let node = node(2, Some("api-2-a1b2c3"))?;
    let webhooks = krad_domain::RunnerWebhooks {
        hooks: Vec::new(),
        targets: vec![crate::runners::webhook_target(&env, &node).ok_or("no target")?],
    };
    let runner: Vec<AppEvent> = (0..events.len())
        .filter_map(|at| krad_agents::runner_record(&webhooks, &events[..at], &events[at]))
        .map(|record| record.event)
        .collect();
    let app = app_events(&node, &events);
    assert_eq!(runner, app);
    assert_eq!(app.len(), 4);
    Ok(())
}
