use krad_domain::{
    CloudRunnerId, EnvironmentData, EnvironmentName, EventKind, OfflineWebhooks, RunnerWebhooks,
    Settings, SyncedWebhook, TerminalNode, Webhook, WebhookSigning, WebhookTarget,
};
use krad_host::runner::{webhooks_dir, webhooks_file};
use krad_host::{FileMode, HostError, HostLink};
use krad_store::{LoginStoreError, Secrets};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::ReadyRunner;

const SYNC_INTERVAL: Duration = Duration::from_secs(2);
const RUNNER_EVENTS: [EventKind; 3] = [
    EventKind::AgentStarted,
    EventKind::AgentWaiting,
    EventKind::AgentFinished,
];

pub(crate) type WebhooksPlan = Option<RunnerWebhooks>;
type BodyError = <String as TryFrom<&'static RunnerWebhooks>>::Error;

#[derive(Debug)]
enum SyncError {
    Host(HostError),
    Body(BodyError),
    Secret(LoginStoreError),
}

impl fmt::Display for SyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host(error) => error.fmt(formatter),
            Self::Body(error) => write!(formatter, "the webhooks could not be written: {error}"),
            Self::Secret(error) => write!(formatter, "a webhook secret could not be read: {error}"),
        }
    }
}

impl From<HostError> for SyncError {
    fn from(error: HostError) -> Self {
        Self::Host(error)
    }
}

impl From<LoginStoreError> for SyncError {
    fn from(error: LoginStoreError) -> Self {
        Self::Secret(error)
    }
}

pub(crate) struct WebhooksWrite {
    runner: CloudRunnerId,
    link: HostLink,
    runner_dir: PathBuf,
    file: PathBuf,
    plan: WebhooksPlan,
}

impl WebhooksWrite {
    #[tracing::instrument(skip_all, fields(runner = %self.runner))]
    pub(crate) fn run(self, secrets: &Secrets) {
        let runner = self.runner;
        if let Err(error) = write_plan(self, secrets) {
            tracing::warn!(%runner, %error, "the runner's webhooks were not synced");
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RunnerWebhooksSync {
    synced: HashMap<CloudRunnerId, WebhooksPlan>,
    checked: Option<Instant>,
}

pub(crate) fn webhook_target(env: &EnvironmentName, node: &TerminalNode) -> Option<WebhookTarget> {
    Some(WebhookTarget {
        process: node.cloud.as_ref()?.id.clone(),
        environment: env.clone(),
        group: node.group,
        terminal: node.id,
        harness: node.harness,
        state: node.state,
    })
}

fn wants_runner_events(hook: &Webhook) -> bool {
    RUNNER_EVENTS.iter().any(|kind| hook.wants(*kind))
}

fn with_secrets(mut plan: RunnerWebhooks, secrets: &Secrets) -> Result<RunnerWebhooks, SyncError> {
    for synced in &mut plan.hooks {
        if synced.hook.signing == WebhookSigning::Signed {
            synced.secret = secrets
                .find_webhook_secret(synced.hook.id)?
                .map(|(secret, _)| secret);
        }
    }
    Ok(plan)
}

fn write_plan(write: WebhooksWrite, secrets: &Secrets) -> Result<(), SyncError> {
    let WebhooksWrite {
        link,
        runner_dir,
        file,
        plan,
        ..
    } = write;
    let Some(plan) = plan else {
        if link.exists(&file) {
            link.remove_file(&file)?;
        }
        return Ok(());
    };
    let text = String::try_from(&with_secrets(plan, secrets)?).map_err(SyncError::Body)?;
    link.create_dir_with_mode(&webhooks_dir(&runner_dir), FileMode::PRIVATE_DIR)?;
    link.write_with_mode(&file, text.as_bytes(), FileMode::PRIVATE_FILE)?;
    Ok(())
}

pub(crate) fn runner_webhooks_plan<'a>(
    runner: CloudRunnerId,
    settings: &Settings,
    environments: impl Iterator<Item = &'a EnvironmentData>,
) -> WebhooksPlan {
    let wanted = settings
        .cloud
        .find(runner)
        .is_some_and(|found| found.offline_webhooks == OfflineWebhooks::Send);
    if !wanted {
        return None;
    }
    let hooks = settings
        .webhooks
        .hooks
        .iter()
        .filter(|hook| wants_runner_events(hook))
        .map(|hook| SyncedWebhook {
            hook: hook.clone(),
            secret: None,
        })
        .collect();
    let targets = environments
        .flat_map(|data| {
            data.nodes
                .iter()
                .filter(move |node| {
                    data.session_of(node.id)
                        .and_then(|session| data.session(session))
                        .and_then(|session| session.cloud.as_ref())
                        .is_some_and(|placement| placement.runner == runner)
                })
                .filter_map(move |node| webhook_target(&data.name, node))
        })
        .collect();
    Some(RunnerWebhooks { hooks, targets })
}

fn site(ready: &ReadyRunner) -> Option<(PathBuf, PathBuf)> {
    let HostLink::Remote(ssh) = &ready.link else {
        return None;
    };
    let runner_dir = ssh.layout()?.runner_dir();
    let file = webhooks_file(&runner_dir, &ready.pairing.install);
    Some((runner_dir, file))
}

impl RunnerWebhooksSync {
    pub(crate) fn is_due(&mut self, now: Instant) -> bool {
        if self
            .checked
            .is_some_and(|checked| now.duration_since(checked) < SYNC_INTERVAL)
        {
            return false;
        }
        self.checked = Some(now);
        true
    }

    pub(crate) fn changed(
        &mut self,
        runner: CloudRunnerId,
        ready: ReadyRunner,
        plan: WebhooksPlan,
    ) -> Option<WebhooksWrite> {
        let (runner_dir, file) = site(&ready)?;
        if self.synced.get(&runner) == Some(&plan) {
            return None;
        }
        self.synced.insert(runner, plan.clone());
        Some(WebhooksWrite {
            runner,
            link: ready.link,
            runner_dir,
            file,
            plan,
        })
    }

    pub(crate) fn check_soon(&mut self) {
        self.checked = None;
    }

    pub(crate) fn resync(&mut self) {
        self.synced.clear();
        self.checked = None;
    }

    pub(crate) fn forget(&mut self, runner: CloudRunnerId) {
        self.synced.remove(&runner);
        self.checked = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_hooks_that_want_agent_events_go_to_the_runner() -> Result<(), Box<dyn std::error::Error>>
    {
        let hook = |kinds: &[EventKind]| -> Result<Webhook, Box<dyn std::error::Error>> {
            Ok(Webhook {
                id: krad_domain::WebhookId(1),
                name: "ci".parse()?,
                url: "https://hooks.example.test/krad".parse()?,
                kinds: kinds.iter().copied().collect(),
                signing: WebhookSigning::Unsigned,
            })
        };
        assert!(wants_runner_events(&hook(&[
            EventKind::GitChanged,
            EventKind::AgentWaiting
        ])?));
        assert!(!wants_runner_events(&hook(&[
            EventKind::CiFailed,
            EventKind::SessionOpened
        ])?));
        Ok(())
    }

    #[test]
    fn a_runner_that_does_not_send_offline_webhooks_gets_no_plan() {
        let settings = Settings::default();
        assert_eq!(
            runner_webhooks_plan(CloudRunnerId(1), &settings, std::iter::empty()),
            None
        );
    }
}
