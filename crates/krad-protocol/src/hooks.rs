use krad_domain::{AgentPreset, AgentState, PermissionMode, WebhookId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::bus::AgentRef;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HookReport {
    pub harness: AgentPreset,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<PermissionMode>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum HookOrigin {
    #[default]
    Live,
    Replayed {
        #[serde(default)]
        delivered: BTreeSet<WebhookId>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conversation {
    #[default]
    Kept,
    StartedFresh,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HookHeard {
    pub agent: AgentRef,
    pub harness: AgentPreset,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<PermissionMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<AgentState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
    #[serde(default)]
    pub conversation: Conversation,
    #[serde(default)]
    pub origin: HookOrigin,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::BusCommand;
    use crate::command::Command;
    use crate::event::{Event, EventKind};
    use crate::ids::Seq;
    use krad_domain::{EnvironmentName, TerminalId};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn agent() -> AgentRef {
        AgentRef {
            environment: EnvironmentName::new("main".to_owned()),
            terminal: TerminalId(4),
        }
    }

    fn report() -> HookReport {
        HookReport {
            harness: AgentPreset::ClaudeCode,
            event: "Stop".to_owned(),
            tool: None,
            detail: None,
            resume: Some("a1b2".to_owned()),
            mode: None,
        }
    }

    #[test]
    fn a_replayed_hook_keeps_the_webhooks_the_runner_delivered() -> TestResult {
        let command = Command::Bus(BusCommand::ReplayHook {
            agent: agent(),
            report: report(),
            delivered: [WebhookId(3)].into_iter().collect(),
        });
        let wire = serde_json::to_value(&command)?;
        assert_eq!(wire["area"], "bus");
        assert_eq!(wire["verb"], "replay_hook");
        assert_eq!(wire["report"]["event"], "Stop");
        assert_eq!(serde_json::from_value::<Command>(wire)?, command);
        Ok(())
    }

    #[test]
    fn a_heard_hook_round_trips_and_an_old_one_reads_as_live_and_kept() -> TestResult {
        let heard = HookHeard {
            agent: agent(),
            harness: AgentPreset::ClaudeCode,
            event: "Notification".to_owned(),
            tool: Some("permission_prompt".to_owned()),
            mode: Some(PermissionMode::Plan),
            state: Some(AgentState::WaitingForInput),
            activity: Some("needs permission".to_owned()),
            conversation: Conversation::StartedFresh,
            origin: HookOrigin::Replayed {
                delivered: [WebhookId(1)].into_iter().collect(),
            },
        };
        let event = Event {
            seq: Seq(2),
            kind: EventKind::AgentHooked(heard),
        };
        let text = serde_json::to_string(&event)?;
        assert_eq!(serde_json::from_str::<Event>(&text)?, event);
        let bare = serde_json::json!({
            "seq": 3,
            "kind": {
                "type": "agent_hooked",
                "agent": {"environment": "main", "terminal": 4},
                "harness": "ClaudeCode",
                "event": "Stop"
            }
        });
        let Event {
            kind: EventKind::AgentHooked(old),
            ..
        } = serde_json::from_value(bare)?
        else {
            return Err("not a hook".into());
        };
        assert_eq!(old.origin, HookOrigin::Live);
        assert_eq!(old.conversation, Conversation::Kept);
        Ok(())
    }
}
