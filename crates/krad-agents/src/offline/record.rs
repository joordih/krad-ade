use krad_domain::{
    agent_event, hook_state, AgentState, EventRecord, RunnerWebhooks, EVENT_API_VERSION,
};

use super::QueuedEvent;
use crate::protocol::Request;

fn state_of(queued: &QueuedEvent) -> Option<AgentState> {
    let Request::Hook {
        harness,
        event,
        tool,
        ..
    } = &queued.request
    else {
        return None;
    };
    hook_state(*harness, event, tool.as_deref())
}

#[must_use]
pub fn runner_record(
    webhooks: &RunnerWebhooks,
    earlier: &[QueuedEvent],
    queued: &QueuedEvent,
) -> Option<EventRecord> {
    let target = webhooks.target(&queued.process)?;
    let Request::Hook { harness, .. } = &queued.request else {
        return None;
    };
    let after = state_of(queued)?;
    let before = earlier
        .iter()
        .filter(|earlier| earlier.process == queued.process)
        .filter_map(state_of)
        .next_back()
        .unwrap_or(target.state);
    let event = agent_event(
        target.terminal,
        target.harness.or(Some(*harness)),
        before,
        after,
    )?;
    Some(EventRecord {
        api: EVENT_API_VERSION,
        at: queued.at,
        environment: target.environment.clone(),
        group: target.group,
        event,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offline::queued_events;
    use krad_domain::{
        AgentPreset, AppEvent, EnvironmentName, GroupId, TerminalId, WebhookTarget,
    };

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const STOP: &str = r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:00.000Z","request":{"Hook":{"harness":"claude","event":"Stop"}}}"#;
    const PROMPT: &str = r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:01.000Z","request":{"Hook":{"harness":"claude","event":"UserPromptSubmit"}}}"#;
    const OTHER: &str = r#"{"process":"web-1-ffffff","at":"2026-11-12T10:00:02.000Z","request":{"Hook":{"harness":"claude","event":"Stop"}}}"#;

    fn webhooks(state: AgentState) -> Result<RunnerWebhooks, Box<dyn std::error::Error>> {
        Ok(RunnerWebhooks {
            hooks: Vec::new(),
            targets: vec![WebhookTarget {
                process: "api-2-a1b2c3".parse()?,
                environment: EnvironmentName::new("api".to_owned()),
                group: Some(GroupId(3)),
                terminal: TerminalId(2),
                harness: Some(AgentPreset::ClaudeCode),
                state,
            }],
        })
    }

    #[test]
    fn a_hook_becomes_the_record_of_its_target_from_the_state_before_it() -> TestResult {
        let lines = queued_events(&[STOP, STOP, PROMPT, OTHER].join("\n"));
        let working = webhooks(AgentState::Working)?;
        let record = runner_record(&working, &[], &lines[0]).ok_or("no record")?;
        assert_eq!(
            record.event,
            AppEvent::AgentFinished {
                terminal: TerminalId(2)
            }
        );
        assert_eq!(record.environment.as_str(), "api");
        assert_eq!(record.group, Some(GroupId(3)));
        assert_eq!(record.at, lines[0].at);
        assert_eq!(runner_record(&working, &lines[..1], &lines[1]), None);
        assert_eq!(
            runner_record(&working, &lines[..2], &lines[2]).map(|record| record.event),
            Some(AppEvent::AgentStarted {
                terminal: TerminalId(2),
                harness: Some(AgentPreset::ClaudeCode)
            })
        );
        assert_eq!(
            runner_record(&webhooks(AgentState::Done)?, &[], &lines[0]),
            None
        );
        assert_eq!(runner_record(&working, &[], &lines[3]), None);
        Ok(())
    }
}
