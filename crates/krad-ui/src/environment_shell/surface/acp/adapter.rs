use gpui_kit::Context;
use krad_chat::Runner;
use krad_domain::{app_kept_acp_package, AcpAgent, AcpAgentId, AgentPreset, ManagedAdapter};
use krad_host::HostLink;
use krad_protocol::{Command, HostCommand, KeptAdapter};
use krad_runtime::kept_package;
use std::path::{Path, PathBuf};

use crate::environment_shell::chat::StartError;
use crate::environment_shell::terminal_card::home_dir;
use crate::environment_shell::EnvironmentShell;

const NO_NODE_TEXT: &str =
    "needs Node.js to install itself. Install Node.js (with npm), then send the message again.";
const FAILED_TEXT: &str = "could not install itself:";
const KEPT_UNCHECKED: &str = "Installs itself in Krad ADE's folder on first use";
const KEPT_CHECKING: &str = "Checking its adapter…";
const KEPT_INSTALLING: &str = "Installing its adapter…";
const KEPT_READY: &str = "Ready; Krad ADE keeps its adapter up to date";
const KEPT_UPDATING: &str = "Ready; looking for a newer adapter…";
const KEPT_NO_NODE: &str = "Needs Node.js (with npm) to install its adapter";
const KEPT_FAILED: &str = "Install failed:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct KeptAdapterView {
    pub(in crate::environment_shell) detail: String,
    pub(in crate::environment_shell) retries: bool,
}

pub(in crate::environment_shell) fn kept_adapter_view(adapter: &ManagedAdapter) -> KeptAdapterView {
    let (detail, retries) = match adapter {
        ManagedAdapter::Unchecked => (KEPT_UNCHECKED.to_owned(), false),
        ManagedAdapter::Checking => (KEPT_CHECKING.to_owned(), false),
        ManagedAdapter::Installing => (KEPT_INSTALLING.to_owned(), false),
        ManagedAdapter::Ready {
            updating: false, ..
        } => (KEPT_READY.to_owned(), false),
        ManagedAdapter::Ready { updating: true, .. } => (KEPT_UPDATING.to_owned(), false),
        ManagedAdapter::NoNode => (KEPT_NO_NODE.to_owned(), true),
        ManagedAdapter::Failed(reason) => (format!("{KEPT_FAILED} {reason}"), true),
    };
    KeptAdapterView { detail, retries }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn acp_runner(
        &self,
        link: &HostLink,
        agent: &AcpAgent,
        root: Option<&Path>,
    ) -> Result<(Runner, PathBuf), StartError> {
        let preset = AgentPreset::Acp(agent.id);
        if kept_package(agent.id, link, root).is_none() {
            return self.program_runner(link, preset, agent.program.clone(), root);
        }
        let name = self.settings.acp.harness_name(preset);
        match self.acp_adapters.get(&agent.id) {
            Some(ManagedAdapter::Ready { program, .. }) => {
                let cwd = root
                    .map(Path::to_path_buf)
                    .or_else(|| home_dir().map(PathBuf::from))
                    .unwrap_or_default();
                Ok((Runner::Native(program.clone()), cwd))
            }
            Some(ManagedAdapter::NoNode) => {
                Err(StartError::Adapter(format!("{name} {NO_NODE_TEXT}")))
            }
            Some(ManagedAdapter::Failed(reason)) => Err(StartError::Adapter(format!(
                "{name} {FAILED_TEXT} {reason}"
            ))),
            Some(ManagedAdapter::Checking | ManagedAdapter::Installing) => Err(StartError::Waiting),
            Some(ManagedAdapter::Unchecked) | None => {
                self.runtime
                    .client
                    .post(Command::Host(HostCommand::CheckAcpAdapter {
                        agent: agent.id,
                    }));
                Err(StartError::Waiting)
            }
        }
    }

    pub(in crate::environment_shell) fn acp_program_name(&self, agent: &AcpAgent) -> String {
        match self.acp_adapters.get(&agent.id) {
            Some(ManagedAdapter::Ready { program, .. }) => program.to_string_lossy().into_owned(),
            Some(_) | None => agent.program.clone(),
        }
    }

    pub(in crate::environment_shell) fn kept_adapter(
        &self,
        agent: AcpAgentId,
    ) -> Option<KeptAdapterView> {
        app_kept_acp_package(agent)?;
        let unchecked = ManagedAdapter::default();
        let adapter = self.acp_adapters.get(&agent).unwrap_or(&unchecked);
        Some(kept_adapter_view(adapter))
    }

    pub(in crate::environment_shell) fn retry_acp_adapter(&mut self, agent: AcpAgentId) {
        self.runtime
            .client
            .post(Command::Host(HostCommand::RetryAcpAdapter { agent }));
        self.detect_agents();
    }

    pub(in crate::environment_shell) fn acp_adapter_heard(
        &mut self,
        kept: KeptAdapter,
        cx: &mut Context<Self>,
    ) {
        self.acp_adapters.insert(kept.agent, kept.adapter);
        self.list_acp_models(kept.agent, cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn acp_adapters_replaced(&mut self, kept: Vec<KeptAdapter>) {
        self.acp_adapters = kept
            .into_iter()
            .map(|kept| (kept.agent, kept.adapter))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_blocked_adapter_offers_a_retry_and_each_state_says_where_it_is() {
        let view = |adapter: ManagedAdapter| kept_adapter_view(&adapter);
        assert_eq!(
            view(ManagedAdapter::Installing),
            KeptAdapterView {
                detail: KEPT_INSTALLING.to_owned(),
                retries: false
            }
        );
        assert!(view(ManagedAdapter::NoNode).retries);
        assert_eq!(
            view(ManagedAdapter::Failed("EACCES".to_owned())).detail,
            "Install failed: EACCES"
        );
        assert!(
            !view(ManagedAdapter::Ready {
                program: PathBuf::from("claude-agent-acp"),
                updating: false
            })
            .retries
        );
    }
}
