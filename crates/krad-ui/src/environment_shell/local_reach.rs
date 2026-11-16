use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{FutureExt as _, StreamExt as _};
use krad_client::{Runner, RuntimeClient, TerminalMirror};
use krad_domain::{EnvironmentName, Host, OpenBuffers};
use krad_extensions::host::HostLink;
use krad_protocol::{AgentRef, BusCommand, Command};
use krad_runtime::{
    BusAccess, BusReply, Delegated, HostShell, RuntimeHandle, TerminalAction, TerminalLaunch,
    TerminalPlace, TerminalView,
};
use krad_session::{AnsiPalette, SearchDirection};

use super::runtime_view::RunnerReacher;
use std::collections::HashMap;
use std::sync::Arc;

pub(super) const SERVICE_ONLY_TEXT: &str =
    "This needs the runtime inside the app; it is not available while the app is a client of this computer's runtime service";

pub(super) struct ServiceReach {
    client: Arc<Runner>,
    mirrors: HashMap<AgentRef, TerminalMirror>,
    palette: Option<AnsiPalette>,
}

pub(super) enum LocalReach {
    InProcess(RuntimeHandle),
    Service(ServiceReach),
}

impl ServiceReach {
    pub(super) fn new(client: Arc<Runner>) -> Self {
        Self {
            client,
            mirrors: HashMap::new(),
            palette: None,
        }
    }
}

impl LocalReach {
    pub(super) const fn handle(&self) -> Option<&RuntimeHandle> {
        match self {
            Self::InProcess(handle) => Some(handle),
            Self::Service(_) => None,
        }
    }

    pub(super) fn runner_reacher(&self) -> RunnerReacher {
        match self {
            Self::InProcess(handle) => RunnerReacher::Line(handle.clone()),
            Self::Service(service) => RunnerReacher::Relay(Arc::clone(&service.client)),
        }
    }

    pub(super) fn host_link(&self, host: &Host) -> Option<HostLink> {
        match self {
            Self::InProcess(handle) => Some(handle.host_link(host)),
            Self::Service(_) => host.is_local().then_some(HostLink::Local),
        }
    }

    pub(super) fn host_place(
        &self,
        host: &Host,
        shell: HostShell,
        in_wsl: bool,
    ) -> Option<TerminalPlace> {
        self.handle()
            .map(|handle| handle.host_place(host, shell, in_wsl))
    }

    pub(super) fn open_terminal(
        &self,
        terminal: AgentRef,
        launch: TerminalLaunch,
    ) -> Option<TerminalView> {
        self.handle()
            .map(|handle| handle.open_terminal(terminal, launch))
    }

    pub(super) fn open_sign_in(&self, terminal: AgentRef) -> Option<TerminalView> {
        self.handle().map(|handle| handle.open_sign_in(terminal))
    }

    pub(super) fn bus(&self) -> Option<&BusAccess> {
        self.handle().and_then(RuntimeHandle::bus)
    }

    pub(super) fn withdraw_discovery(&self) {
        if let Some(handle) = self.handle() {
            handle.withdraw_discovery();
        }
    }

    pub(super) fn takes_delegations(&self) -> bool {
        match self {
            Self::InProcess(handle) => handle.bus().is_some(),
            Self::Service(_) => true,
        }
    }

    pub(super) fn delegations(&self) -> BoxStream<'static, Delegated> {
        match self {
            Self::InProcess(handle) => handle.delegate_bus().boxed(),
            Self::Service(service) => {
                let client = Arc::clone(&service.client);
                service
                    .client
                    .take_delegations()
                    .map(move |delegated| {
                        let client = Arc::clone(&client);
                        let delegation = delegated.delegation;
                        Delegated {
                            from: delegated.from,
                            request: delegated.request,
                            reply: BusReply::remote(move |response| {
                                client.post(Command::Bus(BusCommand::Respond {
                                    delegation,
                                    response,
                                }));
                                true
                            }),
                        }
                    })
                    .boxed()
            }
        }
    }

    pub(super) fn take_launched_view(&mut self, terminal: &AgentRef) -> Option<TerminalView> {
        match self {
            Self::InProcess(handle) => handle.take_launched_view(terminal),
            Self::Service(service) => {
                if let Some(mirror) = service.mirrors.get(terminal) {
                    return Some(mirror.view());
                }
                let palette = service.palette.unwrap_or(krad_session::XTERM_PALETTE);
                let client: Arc<dyn RuntimeClient> = service.client.clone();
                let (mirror, _watched) = TerminalMirror::watch(client, terminal.clone(), palette)
                    .inspect_err(
                        |error| tracing::warn!(%error, "a service terminal could not be mirrored"),
                    )
                    .ok()?;
                let view = mirror.view();
                service.mirrors.insert(terminal.clone(), mirror);
                Some(view)
            }
        }
    }

    pub(super) fn forget_terminal(&mut self, terminal: &AgentRef) {
        if let Self::Service(service) = self {
            service.mirrors.remove(terminal);
        }
    }

    pub(super) fn offer_open_buffers(
        &self,
        environment: EnvironmentName,
        buffers: Arc<dyn OpenBuffers>,
    ) {
        if let Some(handle) = self.handle() {
            handle.offer_open_buffers(environment, buffers);
        }
    }

    pub(super) fn act_on_terminal(&self, terminal: AgentRef, action: TerminalAction) {
        match self {
            Self::InProcess(handle) => handle.act_on_terminal(terminal, action),
            Self::Service(service) => {
                let Some(mirror) = service.mirrors.get(&terminal) else {
                    return tracing::debug!("an action reached a terminal that is not mirrored");
                };
                mirror.act(action);
            }
        }
    }

    pub(super) fn terminal_selection(
        &self,
        terminal: AgentRef,
    ) -> BoxFuture<'static, Option<String>> {
        let (text, answer) = oneshot::channel();
        self.act_on_terminal(terminal, TerminalAction::CopySelection(text));
        answer.map(|text| text.ok().flatten()).boxed()
    }

    pub(super) fn search_terminal(
        &self,
        terminal: AgentRef,
        query: String,
        direction: SearchDirection,
        restart: bool,
    ) -> BoxFuture<'static, bool> {
        let (found, answer) = oneshot::channel();
        self.act_on_terminal(
            terminal,
            TerminalAction::Search {
                query,
                direction,
                restart,
                found,
            },
        );
        answer.map(|found| found.unwrap_or(false)).boxed()
    }

    pub(super) fn terminal_palette(&mut self, palette: AnsiPalette) {
        match self {
            Self::InProcess(handle) => handle.terminal_palette(palette),
            Self::Service(service) => {
                service.palette = Some(palette);
                for mirror in service.mirrors.values() {
                    mirror.recolor(palette);
                }
            }
        }
    }
}
