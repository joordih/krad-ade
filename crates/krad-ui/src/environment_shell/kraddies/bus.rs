use gpui_kit::Context;
use krad_domain::{AutomationKey, KraddieId};
use krad_protocol::KraddieCommand;

use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn run_automation_now(
        &mut self,
        key: &AutomationKey,
        cx: &mut Context<Self>,
    ) {
        let command = KraddieCommand::Run {
            automation: key.clone(),
            prompt: None,
        };
        self.tell_kraddies(command, cx);
    }

    pub(in crate::environment_shell) fn run_kraddie_now(
        &mut self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) {
        let command = KraddieCommand::RunKraddie {
            kraddie: id.clone(),
            prompt: None,
        };
        self.tell_kraddies(command, cx);
    }
}
