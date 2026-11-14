use futures::channel::oneshot;
use krad_domain::{CloudRunnerId, EnvironmentName, Host, RunnerCli};
use krad_host::mux::PromptSender;
use krad_host::{HostLink, MachineChannel};
use krad_protocol::AgentRef;
use std::sync::Arc;

use super::{Pending, RuntimeHandle};
use crate::input::{Input, RunnerInput};
use crate::runners::{RunnerLine, RunnerLineError};
use crate::terminals::{HostShell, Shared, TerminalPlace, TerminalView};

impl RuntimeHandle {
    #[must_use]
    pub fn host_link(&self, host: &Host) -> HostLink {
        self.links.link(host)
    }

    #[must_use]
    pub fn host_place(&self, host: &Host, shell: HostShell, in_wsl: bool) -> TerminalPlace {
        TerminalPlace::Host {
            link: self.links.link(host),
            shell,
            in_wsl,
        }
    }

    #[must_use]
    pub fn prompt_sender(&self) -> Option<PromptSender> {
        self.links.prompts()
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
    pub fn open_runner_ssh_sign_in(
        &self,
        terminal: AgentRef,
        runner: CloudRunnerId,
    ) -> TerminalView {
        let shared = Shared::new(self.inbox.clone());
        let view = TerminalView::new(Arc::clone(&shared));
        self.deliver(Input::Runner(RunnerInput::OpenSshSignIn {
            terminal,
            runner,
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

    pub fn connect_runner(
        &self,
        runner: CloudRunnerId,
    ) -> Pending<Result<RunnerLine, RunnerLineError>> {
        let (reply, answer) = oneshot::channel();
        self.deliver(Input::Runner(RunnerInput::Connect { runner, reply }));
        Pending(answer)
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
}
