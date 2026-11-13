use gpui_kit::Context;
use krad_agents::{ErrorCode, Request, Response};
use krad_domain::KraddieId;
use krad_runtime::BusReply;

use super::super::EnvironmentShell;

const ANSWERED_BY_RUNTIME: &str = "Kraddie and memory requests are answered by the runtime";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn answer_kraddie_request(
        &mut self,
        request: Request,
        reply: BusReply,
        cx: &mut Context<Self>,
    ) {
        let response = match request {
            Request::NewKraddieChat { name } => self.kraddie_chat_from_bus(&name, cx),
            _ => Response::error(ErrorCode::Unsupported, ANSWERED_BY_RUNTIME),
        };
        reply.send(response).unwrap_or_default();
    }

    pub(super) fn kraddie_by_name(&self, name: &str) -> Result<KraddieId, Response> {
        self.kraddies
            .entries
            .iter()
            .find(|entry| entry.kraddie.name.matches(name))
            .map(|entry| entry.kraddie.id())
            .ok_or_else(|| Response::error(ErrorCode::NotFound, format!("No Kraddie named {name}")))
    }
}
