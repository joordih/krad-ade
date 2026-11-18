use futures::channel::oneshot;
use krad_domain::{CloudRunnerId, EnvironmentName, Host};
use krad_host::mux::PromptSender;
use krad_host::{HostLink, MachineChannel};

use super::{Pending, RuntimeHandle};
use crate::input::{Input, RunnerInput};
use crate::runners::{RunnerLine, RunnerLineError};
use crate::terminals::{HostShell, TerminalPlace};

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
