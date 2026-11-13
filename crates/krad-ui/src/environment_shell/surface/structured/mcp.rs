use gpui_kit::{AnyElement, Context};
use krad_domain::TerminalId;
use krad_protocol::{ChatCommand, Command};

use super::super::super::EnvironmentShell;
use super::super::{Backend, SurfaceChat};
use super::runtime_chat::chat_ref;

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn mcp_tray(
        &self,
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let Backend::Structured(structured) = &chat.backend else {
            return None;
        };
        let asked = structured.shown.mcp_trust.as_ref()?;
        let chat = chat_ref(&self.active, id);
        Some(Self::mcp_trust_tray(
            asked,
            move |this, trust, _, _| {
                let trust = ChatCommand::TrustMcp {
                    chat: chat.clone(),
                    trust,
                };
                this.runtime.handle.post(Command::Chat(trust));
            },
            cx,
        ))
    }
}
