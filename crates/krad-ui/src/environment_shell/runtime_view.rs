use gpui_kit::{App, AppContext as _, Context, Entity};
use krad_client::{InProcess, RuntimeClient};
use krad_domain::{AgentStatus, EnvironmentName, RuntimeId, TerminalId};
use krad_protocol::AgentRef;
use krad_runtime::RuntimeHandle;
use krad_session::AnsiPalette;
use std::sync::Arc;

mod asking;
mod environments;
mod following;
mod heard;
mod hosts;
mod mail;
mod settings;
mod view_model;

use asking::Waiters;
pub(super) use environments::EnvironmentsMirror;
pub(super) use hosts::{HostMove, HostsMirror};
pub(super) use mail::MailNotices;
use view_model::RuntimeNews;
pub(super) use view_model::{HostedFailure, RunnerReach, ViewModel};

use super::EnvironmentShell;

type AgentSpot = (EnvironmentName, TerminalId);

fn spot(agent: AgentRef) -> AgentSpot {
    (agent.environment, agent.terminal)
}

pub(super) struct RuntimeLink {
    pub(super) client: Arc<dyn RuntimeClient>,
    pub(super) handle: RuntimeHandle,
    pub(super) view: Entity<ViewModel>,
    pub(super) environments: EnvironmentsMirror,
    palette: Option<AnsiPalette>,
}

impl RuntimeLink {
    pub(super) fn new(client: InProcess, cx: &mut App) -> Self {
        Self {
            handle: client.handle().clone(),
            client: Arc::new(client),
            view: cx.new(|_| ViewModel::default()),
            environments: EnvironmentsMirror::default(),
            palette: None,
        }
    }

    pub(super) fn use_palette(&mut self, palette: AnsiPalette) {
        if self.palette.replace(palette) != Some(palette) {
            self.handle.terminal_palette(palette);
        }
    }
}

impl EnvironmentShell {
    pub(super) fn follow_runtime(&mut self, cx: &mut Context<Self>) {
        let client = Arc::clone(&self.runtime.client);
        self.runtime
            .view
            .update(cx, |_, cx| ViewModel::follow(client, cx));
        cx.subscribe(&self.runtime.view, |shell, _, _: &RuntimeNews, cx| {
            shell.runtime_news(cx);
        })
        .detach();
    }

    fn reach_ready_runners(&self, cx: &mut Context<Self>) {
        let handle = self.runtime.handle.clone();
        self.runtime
            .view
            .update(cx, |model, cx| model.reach_ready_runners(&handle, cx));
    }

    pub(super) fn client_for(
        &self,
        runtime: RuntimeId,
        cx: &App,
    ) -> Option<Arc<dyn RuntimeClient>> {
        self.runtime
            .view
            .read(cx)
            .remotes
            .route(&self.runtime.client, runtime)
    }

    pub(super) fn agent_status<'a>(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &'a App,
    ) -> Option<&'a AgentStatus> {
        self.runtime.view.read(cx).statuses.get(&(env.clone(), id))
    }
}
