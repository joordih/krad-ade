use gpui_kit::Context;
use krad_domain::KraddieId;
use krad_protocol::KraddieCommand;

use super::super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn reset_kraddie_conversation(
        &mut self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) {
        self.kraddies.profile.resetting = None;
        let command = KraddieCommand::ResetConversation {
            kraddie: id.clone(),
        };
        self.tell_kraddies(command, cx);
        cx.notify();
    }
}
