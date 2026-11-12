use serde::{Deserialize, Serialize};

use super::CloudProcessId;
use crate::event::{EventKind, Webhook, WebhookSecret};
use crate::ids::{EnvironmentName, GroupId, TerminalId};
use crate::{AgentPreset, AgentState};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncedWebhook {
    #[serde(flatten)]
    pub hook: Webhook,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<WebhookSecret>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookTarget {
    pub process: CloudProcessId,
    pub environment: EnvironmentName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupId>,
    pub terminal: TerminalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<AgentPreset>,
    pub state: AgentState,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerWebhooks {
    #[serde(default)]
    pub hooks: Vec<SyncedWebhook>,
    #[serde(default)]
    pub targets: Vec<WebhookTarget>,
}

impl RunnerWebhooks {
    #[must_use]
    pub fn target(&self, process: &CloudProcessId) -> Option<&WebhookTarget> {
        self.targets
            .iter()
            .find(|target| target.process == *process)
    }

    pub fn wanting(&self, kind: EventKind) -> impl Iterator<Item = &SyncedWebhook> {
        self.hooks
            .iter()
            .filter(move |synced| kind != EventKind::Ping && synced.hook.wants(kind))
    }
}

impl TryFrom<&RunnerWebhooks> for String {
    type Error = serde_json::Error;

    fn try_from(webhooks: &RunnerWebhooks) -> Result<Self, Self::Error> {
        serde_json::to_string(webhooks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{WebhookId, WebhookSigning};

    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

    fn synced(id: u64, kinds: &[EventKind], secret: Option<&str>) -> TestResult<SyncedWebhook> {
        Ok(SyncedWebhook {
            hook: Webhook {
                id: WebhookId(id),
                name: "ci".parse()?,
                url: "https://hooks.example.test/krad".parse()?,
                kinds: kinds.iter().copied().collect(),
                signing: if secret.is_some() {
                    WebhookSigning::Signed
                } else {
                    WebhookSigning::Unsigned
                },
            },
            secret: secret.and_then(WebhookSecret::parse),
        })
    }

    #[test]
    fn the_runner_file_round_trips_with_its_secrets_and_targets() -> TestResult {
        let file = RunnerWebhooks {
            hooks: vec![
                synced(1, &[EventKind::AgentFinished], Some("s3cr3t"))?,
                synced(2, &[EventKind::GitChanged], None)?,
            ],
            targets: vec![WebhookTarget {
                process: "api-2-a1b2c3".parse()?,
                environment: EnvironmentName::new("api".to_owned()),
                group: Some(GroupId(3)),
                terminal: TerminalId(2),
                harness: Some(AgentPreset::ClaudeCode),
                state: AgentState::Working,
            }],
        };
        let text = String::try_from(&file)?;
        assert!(text.contains(r#""secret":"s3cr3t""#));
        assert_eq!(serde_json::from_str::<RunnerWebhooks>(&text)?, file);
        assert_eq!(
            file.target(&"api-2-a1b2c3".parse()?).map(|t| t.terminal),
            Some(TerminalId(2))
        );
        assert_eq!(file.target(&"web-1-ffffff".parse()?), None);
        let wanting: Vec<WebhookId> = file
            .wanting(EventKind::AgentFinished)
            .map(|synced| synced.hook.id)
            .collect();
        assert_eq!(wanting, vec![WebhookId(1)]);
        assert_eq!(file.wanting(EventKind::Ping).count(), 0);
        assert_eq!(format!("{:?}", file.hooks[0].secret), "Some([REDACTED])");
        Ok(())
    }
}
