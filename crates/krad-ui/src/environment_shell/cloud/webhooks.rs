use gpui_kit::{AppContext as _, Context};
use krad_domain::{
    CloudRunnerId, EnvironmentName, EventKind, OfflineWebhooks, RunnerWebhooks, SyncedWebhook,
    TerminalNode, Webhook, WebhookSigning, WebhookTarget,
};
use krad_host::runner::{webhooks_dir, webhooks_file};
use krad_host::{FileMode, HostError, HostLink};
use krad_store::{find_webhook_secret, LoginStoreError};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::super::EnvironmentShell;

const SYNC_INTERVAL: Duration = Duration::from_secs(2);
const RUNNER_EVENTS: [EventKind; 3] = [
    EventKind::AgentStarted,
    EventKind::AgentWaiting,
    EventKind::AgentFinished,
];

pub(super) type WebhooksPlan = Option<RunnerWebhooks>;
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

pub(super) fn webhook_target(env: &EnvironmentName, node: &TerminalNode) -> Option<WebhookTarget> {
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

fn with_secrets(mut plan: RunnerWebhooks) -> Result<RunnerWebhooks, SyncError> {
    for synced in &mut plan.hooks {
        if synced.hook.signing == WebhookSigning::Signed {
            synced.secret = find_webhook_secret(synced.hook.id)?.map(|(secret, _)| secret);
        }
    }
    Ok(plan)
}

#[tracing::instrument(skip_all, fields(%runner))]
fn write_plan(
    runner: CloudRunnerId,
    link: &HostLink,
    runner_dir: &Path,
    file: &Path,
    plan: WebhooksPlan,
) -> Result<(), SyncError> {
    let Some(plan) = plan else {
        if link.exists(file) {
            link.remove_file(file)?;
        }
        return Ok(());
    };
    let text = String::try_from(&with_secrets(plan)?).map_err(SyncError::Body)?;
    link.create_dir_with_mode(&webhooks_dir(runner_dir), FileMode::PRIVATE_DIR)?;
    link.write_with_mode(file, text.as_bytes(), FileMode::PRIVATE_FILE)?;
    Ok(())
}

impl EnvironmentShell {
    fn runner_webhooks_plan(&self, runner: CloudRunnerId) -> WebhooksPlan {
        let wanted = self
            .settings
            .cloud
            .find(runner)
            .is_some_and(|found| found.offline_webhooks == OfflineWebhooks::Send);
        if !wanted {
            return None;
        }
        let hooks = self
            .settings
            .webhooks
            .hooks
            .iter()
            .filter(|hook| wants_runner_events(hook))
            .map(|hook| SyncedWebhook {
                hook: hook.clone(),
                secret: None,
            })
            .collect();
        let targets = self
            .envs
            .iter()
            .flat_map(|(env, live)| {
                let data = &live.data;
                data.nodes
                    .iter()
                    .filter(move |node| {
                        data.session_of(node.id)
                            .and_then(|session| data.session(session))
                            .and_then(|session| session.cloud.as_ref())
                            .is_some_and(|placement| placement.runner == runner)
                    })
                    .filter_map(move |node| webhook_target(env, node))
            })
            .collect();
        Some(RunnerWebhooks { hooks, targets })
    }

    fn runner_webhooks_site(&self, runner: CloudRunnerId) -> Option<(HostLink, PathBuf, PathBuf)> {
        let link = self.runner_link(runner)?;
        let pairing = self.runner_pairing(runner)?;
        let HostLink::Remote(ssh) = &link else {
            return None;
        };
        let runner_dir = ssh.layout()?.runner_dir();
        let file = webhooks_file(&runner_dir, &pairing.install);
        Some((link, runner_dir, file))
    }

    pub(in crate::environment_shell) fn sync_runner_webhooks(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if self
            .cloud
            .webhooks_checked
            .is_some_and(|checked| now.duration_since(checked) < SYNC_INTERVAL)
        {
            return;
        }
        self.cloud.webhooks_checked = Some(now);
        let runners: Vec<CloudRunnerId> = self
            .settings
            .cloud
            .runners
            .iter()
            .map(|runner| runner.id)
            .collect();
        for runner in runners {
            let Some((link, runner_dir, file)) = self.runner_webhooks_site(runner) else {
                continue;
            };
            let plan = self.runner_webhooks_plan(runner);
            if self.cloud.webhooks.get(&runner) == Some(&plan) {
                continue;
            }
            self.cloud.webhooks.insert(runner, plan.clone());
            cx.background_spawn(async move {
                if let Err(error) = write_plan(runner, &link, &runner_dir, &file, plan) {
                    tracing::warn!(%runner, %error, "the runner's webhooks were not synced");
                }
            })
            .detach();
        }
    }

    pub(in crate::environment_shell) fn resync_runner_webhooks(&mut self) {
        self.cloud.webhooks.clear();
        self.cloud.webhooks_checked = None;
    }

    pub(super) fn forget_runner_webhooks(&mut self, runner: CloudRunnerId) {
        self.cloud.webhooks.remove(&runner);
        self.cloud.webhooks_checked = None;
    }

    pub(in crate::environment_shell) fn set_runner_offline_webhooks(
        &mut self,
        runner: CloudRunnerId,
        wanted: OfflineWebhooks,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.cloud.set_offline_webhooks(runner, wanted) {
            return;
        }
        self.save_settings(cx);
        self.forget_runner_webhooks(runner);
        cx.notify();
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
}
