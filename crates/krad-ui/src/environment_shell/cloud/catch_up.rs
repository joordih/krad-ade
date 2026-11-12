use krad_agents::{QueuedEvent, Request, TerminalHandle};
use krad_domain::{
    AgentPreset, AttentionKind, CloudProcessId, EnvironmentName, PermissionMode, TerminalId,
    TerminalNode, ToolKind,
};
use std::time::Instant;

use super::super::agent_hooks::HookReport;
use super::super::EnvironmentShell;
use super::NodeKey;

#[derive(Debug, PartialEq, Eq)]
enum Replay {
    Hook {
        harness: AgentPreset,
        event: String,
        tool: Option<String>,
        detail: Option<String>,
        resume: Option<String>,
        mode: Option<PermissionMode>,
    },
    Attention(AttentionKind),
}

fn ask_attention(tool: &str) -> AttentionKind {
    match ToolKind::of_claude_code(tool) {
        ToolKind::Question | ToolKind::Plan => AttentionKind::Ask,
        _ => AttentionKind::Permission,
    }
}

fn replay_of(request: Request) -> Option<Replay> {
    match request {
        Request::Hook {
            harness,
            event,
            tool,
            detail,
            resume,
            mode,
        } => Some(Replay::Hook {
            harness,
            event,
            tool,
            detail,
            resume,
            mode,
        }),
        Request::Ask { tool, .. } => Some(Replay::Attention(ask_attention(&tool))),
        _ => None,
    }
}

fn node_of_process<'a>(
    environments: impl IntoIterator<Item = (&'a EnvironmentName, &'a [TerminalNode])>,
    process: &CloudProcessId,
) -> Option<NodeKey> {
    environments.into_iter().find_map(|(env, nodes)| {
        nodes
            .iter()
            .find(|node| {
                node.cloud
                    .as_ref()
                    .is_some_and(|cloud| cloud.id == *process)
            })
            .map(|node| (env.clone(), node.id))
    })
}

impl EnvironmentShell {
    pub(super) fn catch_up(&mut self, events: Vec<QueuedEvent>, started: Instant) {
        for event in events {
            let environments = self
                .envs
                .iter()
                .map(|(env, live)| (env, live.data.nodes.as_slice()));
            let Some(node) = node_of_process(environments, &event.process) else {
                continue;
            };
            let heard_since = self
                .cloud
                .heard
                .get(&node)
                .is_some_and(|heard| *heard >= started);
            if heard_since {
                continue;
            }
            if let Some(replay) = replay_of(event.request) {
                self.replay(&node.0, node.1, replay);
            }
        }
    }

    fn replay(&mut self, env: &EnvironmentName, id: TerminalId, replay: Replay) {
        match replay {
            Replay::Attention(kind) => self.raise_attention(env, id, kind),
            Replay::Hook {
                harness,
                event,
                tool,
                detail,
                resume,
                mode,
            } => {
                let from = TerminalHandle {
                    environment: env.as_str().to_owned(),
                    terminal: id.as_u64(),
                };
                let report = HookReport {
                    harness,
                    event: &event,
                    tool: tool.as_deref(),
                    detail,
                    resume: resume.as_deref(),
                    mode,
                };
                self.apply_hook(&from, report);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::app_events::agent_event;
    use super::*;
    use krad_agents::queued_events;
    use krad_domain::{AgentState, AppEvent, CloudProcess, JournalOffset, NodeKind};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn node(id: u64, process: Option<&str>) -> Result<TerminalNode, Box<dyn std::error::Error>> {
        Ok(TerminalNode {
            id: TerminalId(id),
            title: "Claude".to_owned(),
            state: AgentState::Working,
            harness: Some(AgentPreset::ClaudeCode),
            resume: None,
            group: None,
            text_scale: krad_domain::TextScale::default(),
            kind: NodeKind::Terminal,
            file: None,
            extension: None,
            bookmarks: krad_domain::ChatBookmarks::default(),
            usage: krad_domain::ChatUsage::default(),
            checkpoints: krad_domain::ChatCheckpoints::default(),
            choice: krad_domain::ChatChoice::default(),
            cloud: match process {
                Some(process) => Some(CloudProcess {
                    id: process.parse()?,
                    offset: JournalOffset::default(),
                }),
                None => None,
            },
        })
    }

    #[test]
    fn a_queued_event_finds_its_cloud_node_by_process_id() -> TestResult {
        let (api, web) = (
            EnvironmentName::new("api".to_owned()),
            EnvironmentName::new("web".to_owned()),
        );
        let api_nodes = [node(1, None)?, node(2, Some("api-2-a1b2c3"))?];
        let web_nodes = [node(2, Some("web-2-d4e5f6"))?];
        let environments = [(&api, &api_nodes[..]), (&web, &web_nodes[..])];
        let found = |process: &str| -> Result<Option<NodeKey>, Box<dyn std::error::Error>> {
            Ok(node_of_process(environments, &process.parse()?))
        };
        assert_eq!(found("web-2-d4e5f6")?, Some((web.clone(), TerminalId(2))));
        assert_eq!(found("api-2-a1b2c3")?, Some((api.clone(), TerminalId(2))));
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
        let Replay::Hook {
            harness,
            event,
            tool,
            ..
        } = &replays[0]
        else {
            return Err("the hook was not replayed".into());
        };
        let state = harness
            .hook_signal(event, tool.as_deref())
            .map(AgentState::from)
            .ok_or("the hook carries no state")?;
        assert_eq!(
            agent_event(TerminalId(2), Some(*harness), AgentState::Working, state),
            Some(AppEvent::AgentWaiting {
                terminal: TerminalId(2)
            })
        );
        Ok(())
    }
}
