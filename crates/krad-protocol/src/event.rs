use krad_domain::{
    AgentPreset, AttentionTarget, ChatMessage, EnvironmentChange, EnvironmentData, EnvironmentName,
    EventRecord, GitObjectId, Host, RewindPoint, UsageReport,
};
use serde::{Deserialize, Serialize};

use crate::app_events::WebhookTest;
use crate::attention::{AttentionNotice, AttentionSnapshot};
use crate::bus::{
    AgentRef, AskId, AskSettlement, BusSnapshot, MailCount, PendingAsk, StatusReport,
};
use crate::chats::{ChatAsk, ChatAskId, ChatState, ChatSummary};
use crate::environments::SpendingCrossed;
use crate::gateway::GatewayState;
use crate::harness_health::{HarnessChecked, HealthSnapshot};
use crate::hosts::{HostPrompt, HostState, HostsSnapshot, PromptId, RemoteBusState};
use crate::ids::{RequestId, Seq};
use crate::models::{ListedModels, ModelsOrigin, ModelsSnapshot};
use crate::opencode::{OpencodeShown, OpencodeSnapshot};
use crate::terminals::{TerminalClassified, TerminalSummary};
use crate::usage::{UsageShown, UsageSnapshot};

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Event {
    pub seq: Seq,
    pub kind: EventKind,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    Pong {
        request: RequestId,
    },
    AgentStatus(StatusReport),
    Mail(MailCount),
    AskPending(PendingAsk),
    AskSettled {
        ask: AskId,
        settlement: AskSettlement,
    },
    AppEvent(EventRecord),
    WebhookTested(WebhookTest),
    AttentionRaised(AttentionNotice),
    AttentionLeft {
        target: AttentionTarget,
    },
    AttentionHistoryCleared,
    HarnessChecked(HarnessChecked),
    UsageShown(UsageShown),
    ModelsListed {
        listed: ListedModels,
        origin: ModelsOrigin,
    },
    ModelListFailed {
        host: Host,
        harness: AgentPreset,
        error: String,
    },
    OpencodeServer(OpencodeShown),
    GatewayChanged {
        state: GatewayState,
    },
    GatewayKeyForgotten {
        error: Option<String>,
    },
    EnvironmentOpened {
        environment: Box<EnvironmentData>,
    },
    EnvironmentClosed {
        environment: EnvironmentName,
    },
    EnvironmentsOrdered {
        order: Vec<EnvironmentName>,
    },
    EnvironmentChanged {
        environment: EnvironmentName,
        changes: Vec<EnvironmentChange>,
    },
    SpendingCrossed(SpendingCrossed),
    TerminalChanged {
        terminal: AgentRef,
    },
    TerminalEnded(TerminalSummary),
    TerminalClassified(TerminalClassified),
    HostChanged {
        environment: EnvironmentName,
        state: HostState,
    },
    SignInWanted {
        environment: EnvironmentName,
    },
    MachineChannelWanted {
        environment: EnvironmentName,
    },
    RemoteBusChanged {
        environment: EnvironmentName,
        bus: RemoteBusState,
    },
    HostPrompted(HostPrompt),
    HostPromptSettled {
        prompt: PromptId,
    },
    ChatOpened {
        chat: AgentRef,
        state: Box<ChatState>,
    },
    ChatMessageAdded {
        chat: AgentRef,
        index: usize,
        message: Box<ChatMessage>,
    },
    ChatMessageChanged {
        chat: AgentRef,
        index: usize,
        message: Box<ChatMessage>,
    },
    ChatMessageAppended {
        chat: AgentRef,
        index: usize,
        text: String,
    },
    ChatMessagesCut {
        chat: AgentRef,
        len: usize,
    },
    ChatMessagesPrepended {
        chat: AgentRef,
        count: usize,
    },
    ChatCheckpointed {
        chat: AgentRef,
        point: RewindPoint,
        checkpoint: GitObjectId,
    },
    ChatState {
        chat: AgentRef,
        state: Box<ChatState>,
    },
    ChatAsk(ChatAsk),
    ChatAskSettled {
        chat: AgentRef,
        ask: ChatAskId,
    },
    ChatClosed {
        chat: AgentRef,
    },
    ChatSpent {
        chat: AgentRef,
        report: UsageReport,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Runtime,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub scope: Scope,
    pub seq: Seq,
    #[serde(default)]
    pub bus: BusSnapshot,
    #[serde(default)]
    pub attention: AttentionSnapshot,
    #[serde(default)]
    pub health: HealthSnapshot,
    #[serde(default)]
    pub usage: UsageSnapshot,
    #[serde(default)]
    pub models: ModelsSnapshot,
    #[serde(default)]
    pub environments: Vec<EnvironmentData>,
    #[serde(default)]
    pub terminals: Vec<TerminalSummary>,
    #[serde(default)]
    pub hosts: HostsSnapshot,
    #[serde(default)]
    pub chats: Vec<ChatSummary>,
    #[serde(default)]
    pub opencode: OpencodeSnapshot,
    #[serde(default)]
    pub gateway: GatewayState,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "feed", rename_all = "snake_case")]
pub enum Feed {
    Snapshot(Snapshot),
    Event(Event),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_round_trips_through_json() -> serde_json::Result<()> {
        let event = Event {
            seq: Seq(4),
            kind: EventKind::Pong {
                request: RequestId(9),
            },
        };
        let json = serde_json::to_string(&event)?;
        assert_eq!(json, r#"{"seq":4,"kind":{"type":"pong","request":9}}"#);
        assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        Ok(())
    }

    #[test]
    fn an_event_from_a_newer_runtime_reads_as_unknown_with_its_seq() -> serde_json::Result<()> {
        let event: Event =
            serde_json::from_str(r#"{"seq":7,"kind":{"type":"session_opened","session":3}}"#)?;
        assert_eq!(
            event,
            Event {
                seq: Seq(7),
                kind: EventKind::Unknown,
            }
        );
        Ok(())
    }

    #[test]
    fn a_feed_tells_a_snapshot_from_an_event() -> serde_json::Result<()> {
        let feeds = [
            Feed::Snapshot(Snapshot {
                scope: Scope::Runtime,
                seq: Seq(2),
                bus: BusSnapshot::default(),
                attention: AttentionSnapshot::default(),
                health: HealthSnapshot::default(),
                usage: UsageSnapshot::default(),
                models: ModelsSnapshot::default(),
                environments: Vec::new(),
                terminals: Vec::new(),
                hosts: HostsSnapshot::default(),
                chats: Vec::new(),
                opencode: OpencodeSnapshot::default(),
                gateway: GatewayState::default(),
            }),
            Feed::Event(Event {
                seq: Seq(3),
                kind: EventKind::Pong {
                    request: RequestId(1),
                },
            }),
        ];
        for feed in feeds {
            let json = serde_json::to_string(&feed)?;
            assert_eq!(serde_json::from_str::<Feed>(&json)?, feed);
        }
        Ok(())
    }
}
