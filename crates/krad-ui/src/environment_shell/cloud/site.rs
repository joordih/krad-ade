use gpui_kit::{AppContext as _, Context};
use krad_chat::{stop_cloud_process, CloudRun, Runner};
use krad_domain::{
    AgentPreset, CloudPlacement, CloudRunnerId, EnvironmentName, JournalOffset, Pairing,
    TerminalId, TerminalNode,
};
use krad_host::HostLink;
use std::path::PathBuf;

use super::super::chat::StartError;
use super::super::EnvironmentShell;

#[derive(Clone, Debug)]
pub(in crate::environment_shell) struct CloudSite {
    pub(in crate::environment_shell) link: HostLink,
    pub(in crate::environment_shell) cli: PathBuf,
    pub(in crate::environment_shell) pairing: Pairing,
    pub(in crate::environment_shell) placement: CloudPlacement,
}

#[derive(Clone, Debug)]
pub(in crate::environment_shell) enum NodeSite {
    Env,
    Cloud(CloudSite),
    Waiting(CloudRunnerId),
}

impl NodeSite {
    pub(in crate::environment_shell) const fn is_cloud(&self) -> bool {
        matches!(self, Self::Cloud(_) | Self::Waiting(_))
    }
}

type ReadyRunner = (HostLink, PathBuf, Pairing);

fn site_of(
    placement: Option<CloudPlacement>,
    ready: impl FnOnce(CloudRunnerId) -> Option<ReadyRunner>,
) -> NodeSite {
    let Some(placement) = placement else {
        return NodeSite::Env;
    };
    match ready(placement.runner) {
        Some((link, cli, pairing)) => NodeSite::Cloud(CloudSite {
            link,
            cli,
            pairing,
            placement,
        }),
        None => NodeSite::Waiting(placement.runner),
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn cloud_placement_of(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<CloudPlacement> {
        let data = &self.envs.get(env)?.data;
        data.session(data.session_of(id)?)?.cloud.clone()
    }

    pub(in crate::environment_shell) fn node_site(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> NodeSite {
        site_of(self.cloud_placement_of(env, id), |runner| {
            Some((
                self.runner_link(runner)?,
                self.runner_cli(runner)?,
                self.runner_pairing(runner)?,
            ))
        })
    }

    pub(in crate::environment_shell) fn runner_name(&self, id: CloudRunnerId) -> String {
        self.settings
            .cloud
            .find(id)
            .map_or_else(|| id.to_string(), |runner| runner.name.as_str().to_owned())
    }

    pub(super) fn node_mut(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<&mut TerminalNode> {
        self.envs
            .get_mut(env)?
            .data
            .nodes
            .iter_mut()
            .find(|node| node.id == id)
    }

    pub(in crate::environment_shell) fn cloud_chat_runner(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        site: &CloudSite,
        harness: AgentPreset,
        program: String,
    ) -> Result<(Runner, PathBuf), StartError> {
        let cwd = site.placement.worktree.clone();
        if harness == AgentPreset::Opencode {
            let runner = Runner::Ssh {
                link: site.link.clone(),
                program,
            };
            return Ok((runner, cwd));
        }
        let start = self.cloud_process_start(env, id, site)?;
        let run = CloudRun::new(
            site.link.clone(),
            site.cli.clone(),
            site.pairing.clone(),
            program,
            start.id,
            start.from,
        );
        self.cloud.attached.insert((env.clone(), id), run.clone());
        Ok((Runner::Cloud(run), cwd))
    }

    pub(in crate::environment_shell) fn attached_run(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<&CloudRun> {
        self.cloud.attached.get(&(env.clone(), id))
    }

    pub(in crate::environment_shell) fn rewind_cloud_attach(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> bool {
        let key = (env.clone(), id);
        if !self.cloud.reattached.insert(key.clone()) {
            return false;
        }
        self.cloud.attached.remove(&key);
        if let Some(saved) = self.node_mut(env, id).and_then(|node| node.cloud.as_mut()) {
            saved.offset = JournalOffset::default();
        }
        true
    }

    pub(in crate::environment_shell) fn nodes_with_offsets(
        &self,
        env: &EnvironmentName,
        nodes: &[TerminalNode],
    ) -> Vec<TerminalNode> {
        nodes
            .iter()
            .map(|node| {
                let mut node = node.clone();
                let consumed = self
                    .cloud
                    .attached
                    .get(&(env.clone(), node.id))
                    .map(CloudRun::consumed);
                if let (Some(saved), Some(consumed)) = (node.cloud.as_mut(), consumed) {
                    saved.offset = consumed;
                }
                node
            })
            .collect()
    }

    pub(in crate::environment_shell) fn stop_cloud_node(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        self.end_runner_helper(env, id);
        let key = (env.clone(), id);
        self.cloud.attached.remove(&key);
        self.cloud.reattached.remove(&key);
        self.cloud.liveness.remove(&key);
        self.cloud.heard.remove(&key);
        let Some(process) = self
            .envs
            .get(env)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
            .and_then(|node| node.cloud.clone())
        else {
            return;
        };
        let site = match self.node_site(env, id) {
            NodeSite::Cloud(site) => site,
            NodeSite::Waiting(runner) => return self.queue_cloud_stop(runner, process.id, cx),
            NodeSite::Env => return,
        };
        cx.background_spawn(async move {
            if let Err(error) =
                stop_cloud_process(&site.link, &site.cli, &site.pairing, &process.id)
            {
                tracing::warn!(process = %process.id, %error, "the cloud process did not stop");
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn placement() -> CloudPlacement {
        CloudPlacement {
            runner: CloudRunnerId(5),
            repo_dir: PathBuf::from("/r/repos/api"),
            worktree: PathBuf::from("/r/worktrees/api/api-a1"),
            branch: "krad/api-a1".to_owned(),
        }
    }

    fn pairing() -> Result<Pairing, Box<dyn std::error::Error>> {
        Ok(Pairing {
            install: "0123456789abcdef".parse()?,
            token: "00112233445566778899aabbccddeeff".parse()?,
        })
    }

    #[test]
    fn nodes_outside_cloud_sessions_stay_on_their_environment() {
        let site = site_of(None, |_| unreachable!("no runner is asked for"));
        assert!(matches!(site, NodeSite::Env));
        assert!(!site.is_cloud());
    }

    #[test]
    fn a_cloud_node_waits_for_its_runner_and_then_runs_there() -> TestResult {
        let waiting = site_of(Some(placement()), |_| None);
        assert!(matches!(waiting, NodeSite::Waiting(CloudRunnerId(5))));
        assert!(waiting.is_cloud());
        let pairing = pairing()?;
        let ready = site_of(Some(placement()), |runner| {
            assert_eq!(runner, CloudRunnerId(5));
            Some((HostLink::Local, PathBuf::from("/bin/cli"), pairing))
        });
        let NodeSite::Cloud(site) = ready else {
            return Err("the ready runner was not used".into());
        };
        assert_eq!(
            site.placement.worktree,
            Path::new("/r/worktrees/api/api-a1")
        );
        Ok(())
    }
}
