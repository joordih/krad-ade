use krad_domain::{CloudRunnerId, EnvironmentName, Host, RunnerCli};
use krad_host::mux::PromptSender;
use krad_host::{HostContext, HostLink, MachineChannel};
use krad_protocol::AgentRef;
use std::sync::Arc;

use super::RuntimeHandle;
use crate::input::{Input, RunnerInput};
use crate::runners::ReadyRunner;
use crate::terminals::{Shared, TerminalView};

impl RuntimeHandle {
    #[must_use]
    pub fn host_link(&self, host: &Host) -> HostLink {
        self.links.link(host)
    }

    pub fn use_host_context(&self, context: HostContext) {
        self.links.use_context(context);
    }

    #[must_use]
    pub fn prompt_sender(&self) -> Option<PromptSender> {
        self.links.prompts()
    }

    pub fn close_hosts(&self) {
        self.links.close_all();
    }

    #[must_use]
    pub fn open_sign_in(&self, terminal: AgentRef) -> TerminalView {
        let shared = Shared::new(self.inbox.clone());
        let view = TerminalView::new(Arc::clone(&shared));
        self.deliver(Input::OpenSignIn { terminal, shared });
        view
    }

    #[must_use]
    pub fn open_runner_sign_in(
        &self,
        terminal: AgentRef,
        runner: CloudRunnerId,
        cli: RunnerCli,
    ) -> TerminalView {
        let shared = Shared::new(self.inbox.clone());
        let view = TerminalView::new(Arc::clone(&shared));
        self.deliver(Input::Runner(RunnerInput::OpenSignIn {
            terminal,
            runner,
            cli,
            shared,
        }));
        view
    }

    #[must_use]
    pub fn open_claude_setup_token(&self, terminal: AgentRef) -> TerminalView {
        let shared = Shared::new(self.inbox.clone());
        let view = TerminalView::new(Arc::clone(&shared));
        self.deliver(Input::Runner(RunnerInput::OpenClaudeSetup {
            terminal,
            shared,
        }));
        view
    }

    pub fn hand_machine_channel(
        &self,
        environment: EnvironmentName,
        channel: Result<MachineChannel, String>,
    ) {
        self.deliver(Input::MachineChannel {
            environment,
            channel,
        });
    }

    #[must_use]
    pub fn runner(&self, runner: CloudRunnerId) -> Option<ReadyRunner> {
        self.runners.get(runner)
    }
}
