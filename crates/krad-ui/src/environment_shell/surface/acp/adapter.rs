use gpui_kit::{App, Context};
use krad_domain::{app_kept_acp_package, AcpAgent, AcpAgentId, ManagedAdapter};
use krad_protocol::{Command, HostCommand};

use crate::environment_shell::EnvironmentShell;

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
    pub(in crate::environment_shell) fn acp_program_name(
        &self,
        agent: &AcpAgent,
        cx: &App,
    ) -> String {
        match self.runtime.view.read(cx).adapters.get(&agent.id) {
            Some(ManagedAdapter::Ready { program, .. }) => program.to_string_lossy().into_owned(),
            Some(_) | None => agent.program.clone(),
        }
    }

    pub(in crate::environment_shell) fn kept_adapter(
        &self,
        agent: AcpAgentId,
        cx: &App,
    ) -> Option<KeptAdapterView> {
        app_kept_acp_package(agent)?;
        let unchecked = ManagedAdapter::default();
        let model = self.runtime.view.read(cx);
        let adapter = model.adapters.get(&agent).unwrap_or(&unchecked);
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
        agent: AcpAgentId,
        cx: &mut Context<Self>,
    ) {
        self.list_acp_models(agent, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
