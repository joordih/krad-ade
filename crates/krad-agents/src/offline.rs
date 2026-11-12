mod record;
#[cfg(unix)]
mod send;

pub use record::runner_record;

use crate::protocol::{Request, Timestamp};
use krad_domain::{CloudProcessId, WebhookId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueuedEvent {
    pub process: CloudProcessId,
    pub at: Timestamp,
    pub request: Request,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub webhooked: BTreeSet<WebhookId>,
}

impl QueuedEvent {
    fn new(process: CloudProcessId, at: Timestamp, request: &Request) -> Option<Self> {
        let request = match request {
            hook @ Request::Hook { .. } => hook.clone(),
            Request::Ask { tool, call, .. } => Request::Ask {
                tool: tool.clone(),
                input: Value::Null,
                suggestions: Vec::new(),
                call: call.clone(),
            },
            _ => return None,
        };
        Some(Self {
            process,
            at,
            request,
            webhooked: BTreeSet::new(),
        })
    }
}

#[must_use]
pub fn queued_events(text: &str) -> Vec<QueuedEvent> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

pub(crate) struct OfflineEvent(QueuedEvent);

impl OfflineEvent {
    pub(crate) fn of_this_process(request: &Request) -> Option<Self> {
        let process = std::env::var(krad_host::runner::PROCESS_ENV).ok();
        queued_event(process, request).map(Self)
    }

    #[cfg(unix)]
    pub(crate) fn queue(self) {
        let Some(runner_dir) = krad_host::runner::home_runner_dir() else {
            return;
        };
        let installs = krad_host::runner::paired_installs(&runner_dir);
        let lines = send::queue_lines(&runner_dir, &installs, self.0);
        krad_host::runner::queue_events(&runner_dir, &lines);
    }

    #[cfg(not(unix))]
    pub(crate) fn queue(self) {
        drop(self.0);
    }
}

fn queued_event(process: Option<String>, request: &Request) -> Option<QueuedEvent> {
    let process = process?.parse().ok()?;
    QueuedEvent::new(process, Timestamp::now(), request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::AgentStatus;
    use krad_domain::AgentPreset;

    fn queued_line(process: Option<String>, request: &Request) -> Option<String> {
        serde_json::to_string(&queued_event(process, request)?).ok()
    }

    fn hook() -> Request {
        Request::Hook {
            harness: AgentPreset::ClaudeCode,
            event: "Stop".to_owned(),
            tool: None,
            detail: Some("All tests pass".to_owned()),
            resume: None,
            mode: None,
        }
    }

    #[test]
    fn only_hooks_and_asks_of_a_cloud_process_are_queued() -> Result<(), serde_json::Error> {
        let process = Some("api-4-a1b2c3".to_owned());
        let line = queued_line(process.clone(), &hook()).unwrap_or_default();
        let events = queued_events(&format!("{line}\nnot json\n\n"));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].process.as_str(), "api-4-a1b2c3");
        assert_eq!(events[0].request, hook());
        assert!(events[0].webhooked.is_empty());
        assert!(!line.contains("webhooked"));
        assert_eq!(queued_line(None, &hook()), None);
        assert_eq!(queued_line(Some("Not An Id".to_owned()), &hook()), None);
        let status = Request::Status(AgentStatus::default());
        assert_eq!(queued_line(process.clone(), &status), None);
        let ask = Request::Ask {
            tool: "Bash".to_owned(),
            input: serde_json::json!({ "command": "rm -rf target" }),
            suggestions: vec![serde_json::json!({ "type": "setMode" })],
            call: None,
        };
        let queued: QueuedEvent =
            serde_json::from_str(&queued_line(process, &ask).unwrap_or_default())?;
        assert!(matches!(
            queued.request,
            Request::Ask { tool, input: Value::Null, suggestions, .. }
                if tool == "Bash" && suggestions.is_empty()
        ));
        Ok(())
    }
}
