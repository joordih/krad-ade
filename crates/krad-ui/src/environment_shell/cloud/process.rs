use gpui_kit::{AppContext as _, Context};
use krad_domain::{
    CloudProcess, CloudProcessId, CloudProcessRun, EnvironmentName, JournalOffset, TerminalId,
};
use krad_host::runner::{parse_status, run_authorized, status_command, CloudProcessStatus};
use krad_host::HostError;
use std::process::Output;

use super::super::chat::StartError;
use super::super::EnvironmentShell;
use super::naming::{new_nonce, node_process_id};
use super::{CloudSite, NodeKey};

const NO_PROCESS_ID: &str = "The cloud session's folder gives no process name";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Liveness {
    Checking,
    Running,
    Gone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct ProcessStart {
    pub(in crate::environment_shell) id: CloudProcessId,
    pub(in crate::environment_shell) from: JournalOffset,
    pub(in crate::environment_shell) fresh: bool,
}

pub(super) struct LivenessCheck {
    key: NodeKey,
    site: CloudSite,
    process: CloudProcessId,
}

fn liveness_in(statuses: &[CloudProcessStatus], process: &CloudProcessId) -> Liveness {
    match statuses.iter().find(|status| status.id == *process) {
        Some(status) if status.run == CloudProcessRun::Running => Liveness::Running,
        Some(_) | None => Liveness::Gone,
    }
}

fn listing_of(output: Result<Output, HostError>) -> Result<String, String> {
    let output = output.map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn liveness_from(listing: Result<String, String>, process: &CloudProcessId) -> Liveness {
    let listed = listing.and_then(|text| parse_status(&text).map_err(|error| error.to_string()));
    match listed {
        Ok(statuses) => liveness_in(&statuses, process),
        Err(error) => {
            tracing::debug!(%process, %error, "the runner's processes could not be listed; attaching again");
            Liveness::Running
        }
    }
}

impl EnvironmentShell {
    fn keep_consumed(&mut self, env: &EnvironmentName, id: TerminalId) {
        let Some(run) = self.cloud.attached.remove(&(env.clone(), id)) else {
            return;
        };
        let consumed = run.consumed();
        if let Some(saved) = self
            .node_mut(env, id)
            .and_then(|node| node.cloud.as_mut())
            .filter(|saved| saved.id == run.process)
        {
            saved.offset = consumed;
        }
    }

    fn fresh_process(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        site: &CloudSite,
    ) -> Result<ProcessStart, StartError> {
        let process = node_process_id(&site.placement, id, &new_nonce())
            .ok_or_else(|| StartError::Cloud(NO_PROCESS_ID.to_owned()))?;
        if let Some(node) = self.node_mut(env, id) {
            node.cloud = Some(CloudProcess {
                id: process.clone(),
                offset: JournalOffset::default(),
            });
        }
        Ok(ProcessStart {
            id: process,
            from: JournalOffset::default(),
            fresh: true,
        })
    }

    pub(in crate::environment_shell) fn cloud_process_start(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        site: &CloudSite,
    ) -> Result<ProcessStart, StartError> {
        self.keep_consumed(env, id);
        let key = (env.clone(), id);
        let saved = self
            .envs
            .get(env)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
            .and_then(|node| node.cloud.clone());
        let Some(saved) = saved else {
            return self.fresh_process(env, id, site);
        };
        match self.cloud.liveness.get(&key).copied() {
            None => {
                self.cloud.liveness.insert(key.clone(), Liveness::Checking);
                self.cloud.unchecked.push(LivenessCheck {
                    key,
                    site: site.clone(),
                    process: saved.id,
                });
                Err(StartError::Waiting)
            }
            Some(Liveness::Checking) => Err(StartError::Waiting),
            Some(Liveness::Running) => {
                self.cloud.liveness.remove(&key);
                Ok(ProcessStart {
                    id: saved.id,
                    from: saved.offset,
                    fresh: false,
                })
            }
            Some(Liveness::Gone) => {
                self.cloud.liveness.remove(&key);
                self.fresh_process(env, id, site)
            }
        }
    }

    pub(in crate::environment_shell) fn check_cloud_processes(&mut self, cx: &mut Context<Self>) {
        for LivenessCheck { key, site, process } in std::mem::take(&mut self.cloud.unchecked) {
            let status = status_command(&site.cli, &site.pairing);
            let checked = cx.background_spawn(async move {
                let output = run_authorized(&site.link, &status, &site.pairing);
                liveness_from(listing_of(output), &process)
            });
            cx.spawn(async move |this, cx| {
                let liveness = checked.await;
                this.update(cx, |shell, cx| {
                    shell.cloud_process_checked(&key, liveness, cx);
                })
                .unwrap_or_default();
            })
            .detach();
        }
    }

    fn cloud_process_checked(&mut self, key: &NodeKey, liveness: Liveness, cx: &mut Context<Self>) {
        let Some(known) = self.cloud.liveness.get_mut(key) else {
            return;
        };
        *known = liveness;
        self.resume_cloud_work(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn listed(text: &str) -> Result<String, String> {
        parse_status(text)
            .map(|_| text.to_owned())
            .map_err(|error| error.to_string())
    }

    #[test]
    fn only_a_running_process_is_attached_again() -> TestResult {
        let process: CloudProcessId = "api-a1-7-f00dbe".parse()?;
        let running = listed("api-a1-7-f00dbe\trunning\t120\n");
        assert_eq!(liveness_from(running, &process), Liveness::Running);
        let ended = listed("api-a1-7-f00dbe\tended:0\t120\n");
        assert_eq!(liveness_from(ended, &process), Liveness::Gone);
        let unknown = listed("other-1\trunning\t3\n");
        assert_eq!(liveness_from(unknown, &process), Liveness::Gone);
        Ok(())
    }

    #[test]
    fn an_unreadable_listing_never_starts_a_second_process() -> TestResult {
        let process: CloudProcessId = "api-a1-7-f00dbe".parse()?;
        let failed = listing_of(Err(HostError::NotConnected));
        assert_eq!(liveness_from(failed, &process), Liveness::Running);
        assert_eq!(
            liveness_from(listed("garbage"), &process),
            Liveness::Running
        );
        Ok(())
    }
}
