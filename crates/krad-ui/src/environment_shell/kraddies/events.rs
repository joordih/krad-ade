use gpui_kit::Context;
use krad_domain::{AppEvent, CodeHost, EnvironmentName, GroupId};
use krad_protocol::KraddieCommand;

use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn observe_code_host_event(
        &mut self,
        env: &EnvironmentName,
        group: GroupId,
        host: CodeHost,
        event: &AppEvent,
        cx: &mut Context<Self>,
    ) {
        let command = KraddieCommand::CodeHostSeen {
            environment: env.clone(),
            group,
            host,
            event: event.clone(),
        };
        self.tell_kraddies(command, cx);
    }

    pub(in crate::environment_shell) fn automations_want_code_host(&self) -> bool {
        self.kraddies.entries.iter().any(|kraddie| {
            kraddie.automations.iter().any(|entry| {
                entry.automation.switch.is_on()
                    && entry
                        .automation
                        .triggers
                        .iter()
                        .any(|trigger| trigger.code_host().is_some())
            })
        })
    }
}
