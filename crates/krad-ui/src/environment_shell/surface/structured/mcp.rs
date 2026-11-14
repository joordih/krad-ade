use gpui_kit::{AnyElement, Context};
use krad_domain::TerminalId;
use krad_protocol::ChatCommand;

use super::super::super::EnvironmentShell;
use super::super::{Backend, ChatView};

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn mcp_tray(
        &self,
        id: TerminalId,
        chat: &ChatView,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let Backend::Structured(structured) = &chat.chat.backend else {
            return None;
        };
        let asked = structured.shown.mcp_trust.as_ref()?;
        let place = self.chat_place();
        Some(Self::mcp_trust_tray(
            asked,
            move |this, trust, _, _| {
                let trust = ChatCommand::TrustMcp {
                    chat: place.chat(id),
                    trust,
                };
                this.post_chat(&place, trust);
            },
            cx,
        ))
    }
}
