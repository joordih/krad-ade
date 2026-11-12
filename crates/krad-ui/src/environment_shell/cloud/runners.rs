use gpui_kit::{AppContext as _, Context};
use krad_domain::{CloudRunnerId, CloudRunnerName, Host, SshDestination, ToastKind};
use krad_host::{HostError, HostLink, HostPlatform, SshFailure};
use krad_store::{
    delete_cloud_claude_token, delete_cloud_opencode_password, delete_cloud_runner_token,
};

use super::super::agent_bus::RemoteBusSlot;
use super::super::host::{close_link, explain_host_problem};
use super::super::EnvironmentShell;
use super::install::{prepare_runner, retire_runner, PreparedRunner, RunnerSetupError};
use super::{LiveRunner, RunnerState};

const ADDED_TOAST: &str = "Cloud runner added";
const REMOVED_TOAST: &str = "Cloud runner removed";
const RETIRE_FAILED: &str = "The cloud runner was removed, but cleaning up its host failed";
const TOKEN_NOT_DELETED: &str = "Could not forget the cloud runner's pairing token";

fn explain(error: &HostError, destination: &SshDestination) -> String {
    match error {
        HostError::Ssh(SshFailure::NeedsInteractiveSignIn) => format!(
            "{destination} asks for a password or passphrase. Open an SSH environment on {destination}, sign in there once, then reconnect."
        ),
        HostError::Ssh(SshFailure::HostKeyUnknown) => format!(
            "The key of {destination} is not known yet. Open an SSH environment on {destination} to check and accept it, then reconnect."
        ),
        other => explain_host_problem(other, destination),
    }
}

impl EnvironmentShell {
    fn runner_destination(&self, id: CloudRunnerId) -> Option<SshDestination> {
        self.settings
            .cloud
            .find(id)
            .map(|runner| runner.destination.clone())
    }

    fn live_runner(&mut self, id: CloudRunnerId) -> Option<&mut LiveRunner> {
        let destination = self.runner_destination(id)?;
        let context = self.host_context();
        Some(self.cloud.live.entry(id).or_insert_with(|| LiveRunner {
            link: HostLink::new(&Host::Ssh { destination }, &context),
            state: RunnerState::NotConnected,
            prepared: None,
            bus: RemoteBusSlot::Absent,
            bus_port: None,
        }))
    }

    pub(in crate::environment_shell) fn ensure_runner_ready(
        &mut self,
        id: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live_runner(id) else {
            return;
        };
        let wants_connection = match &live.state {
            RunnerState::NotConnected | RunnerState::Failed(_) => true,
            state if state.is_ready() => !live.link.is_ready(),
            _ => false,
        };
        if wants_connection {
            self.connect_runner(id, cx);
        }
    }

    pub(in crate::environment_shell) fn show_cloud_runners(&mut self, cx: &mut Context<Self>) {
        let waiting: Vec<CloudRunnerId> = self
            .settings
            .cloud
            .runners
            .iter()
            .map(|runner| runner.id)
            .filter(|id| {
                self.cloud
                    .live
                    .get(id)
                    .is_none_or(|live| live.state == RunnerState::NotConnected)
            })
            .collect();
        for id in waiting {
            self.ensure_runner_ready(id, cx);
        }
    }

    pub(in crate::environment_shell) fn reconnect_runner(
        &mut self,
        id: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        if self
            .live_runner(id)
            .is_some_and(|live| !live.state.is_busy())
        {
            self.connect_runner(id, cx);
        }
    }

    fn connect_runner(&mut self, id: CloudRunnerId, cx: &mut Context<Self>) {
        let Some(live) = self.live_runner(id) else {
            return;
        };
        let HostLink::Remote(ssh) = live.link.clone() else {
            return;
        };
        live.state = RunnerState::Connecting;
        live.prepared = None;
        live.bus = RemoteBusSlot::Absent;
        cx.notify();
        let connect = cx.background_spawn(async move { ssh.connect() });
        cx.spawn(async move |this, cx| {
            let outcome = connect.await;
            this.update(cx, |shell, cx| shell.runner_connected(id, outcome, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn runner_connected(
        &mut self,
        id: CloudRunnerId,
        outcome: Result<HostPlatform, HostError>,
        cx: &mut Context<Self>,
    ) {
        let destination = self.runner_destination(id);
        let install_id = self.host_tools.install_id.clone();
        let Some(live) = self.cloud.live.get_mut(&id) else {
            return;
        };
        if live.state != RunnerState::Connecting {
            return;
        }
        cx.notify();
        if let Err(error) = outcome {
            tracing::info!(runner = %id, %error, "cloud runner connection failed");
            let reason = destination.map_or_else(
                || error.to_string(),
                |destination| explain(&error, &destination),
            );
            live.state = RunnerState::Failed(reason);
            self.cloud_runner_settled(id, cx);
            return;
        }
        let link = live.link.clone();
        let HostLink::Remote(ssh) = link.clone() else {
            return;
        };
        live.state = RunnerState::Installing;
        let prepare =
            cx.background_spawn(async move { prepare_runner(&link, &ssh, id, install_id) });
        cx.spawn(async move |this, cx| {
            let outcome = prepare.await;
            this.update(cx, |shell, cx| shell.runner_prepared(id, outcome, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn runner_prepared(
        &mut self,
        id: CloudRunnerId,
        outcome: Result<PreparedRunner, RunnerSetupError>,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.cloud.live.get_mut(&id) else {
            return;
        };
        if live.state != RunnerState::Installing {
            return;
        }
        match outcome {
            Ok(prepared) => {
                live.state = RunnerState::from(prepared.report);
                let token = prepared.claude_token.clone();
                live.prepared = Some(prepared);
                self.keep_runner_claude_token(id, token);
            }
            Err(error) => {
                tracing::warn!(runner = %id, %error, "could not set up the cloud runner");
                live.state = RunnerState::Failed(error.to_string());
            }
        }
        self.cloud_runner_settled(id, cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn add_cloud_runner(
        &mut self,
        name: CloudRunnerName,
        destination: SshDestination,
        cx: &mut Context<Self>,
    ) {
        let id = self.settings.cloud.add(name, destination);
        self.save_settings(cx);
        self.toast(ToastKind::Success, ADDED_TOAST, cx);
        self.ensure_runner_ready(id, cx);
    }

    pub(in crate::environment_shell) fn remove_cloud_runner(
        &mut self,
        id: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        let Some(runner) = self.settings.cloud.remove(id) else {
            return;
        };
        self.save_settings(cx);
        let link = self.runner_link(id);
        let paired = self.runner_cli(id).zip(self.runner_pairing(id));
        let live = self.cloud.live.remove(&id);
        self.forget_runner_logins(id);
        self.forget_runner_webhooks(id);
        self.keep_runner_claude_token(id, None);
        self.mark_runner_removed(id);
        let host = Host::Ssh {
            destination: runner.destination,
        };
        let shared = self.envs.values().any(|env| env.data.host == host);
        cx.notify();
        let removal = cx.background_spawn(async move {
            let failures = link
                .filter(HostLink::is_ready)
                .map_or_else(Vec::new, |link| retire_runner(&link, paired.as_ref()));
            if let (Some(live), false) = (live, shared) {
                close_link(&live.link);
            }
            let forgotten = delete_cloud_runner_token(id)
                .and_then(|()| delete_cloud_opencode_password(id))
                .and_then(|()| delete_cloud_claude_token(id));
            (failures, forgotten)
        });
        cx.spawn(async move |this, cx| {
            let (failures, forgotten): (Vec<HostError>, _) = removal.await;
            this.update(cx, |shell, cx| {
                shell.runner_removed(&failures, forgotten.err(), cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn runner_removed(
        &mut self,
        failures: &[HostError],
        forgotten: Option<krad_store::LoginStoreError>,
        cx: &mut Context<Self>,
    ) {
        if let Some(error) = forgotten {
            self.toast(
                ToastKind::Error,
                &format!("{TOKEN_NOT_DELETED}: {error}"),
                cx,
            );
            return;
        }
        if let Some(error) = failures.first() {
            self.toast(ToastKind::Error, &format!("{RETIRE_FAILED}: {error}"), cx);
            return;
        }
        self.toast(ToastKind::Success, REMOVED_TOAST, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_asking_for_a_password_sends_the_user_to_sign_in_once(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let destination: SshDestination = "me@gpu".parse()?;
        let text = explain(
            &HostError::Ssh(SshFailure::NeedsInteractiveSignIn),
            &destination,
        );
        assert!(text.contains("Open an SSH environment on me@gpu"));
        let other = explain(&HostError::Ssh(SshFailure::NameResolution), &destination);
        assert!(other.contains("Could not find me@gpu"));
        Ok(())
    }
}
