use gpui_kit::Context;
use krad_domain::{Settings, ToastKind};
use krad_protocol::{Command, SettingsCommand};
use std::rc::Rc;

use super::super::EnvironmentShell;

const SETTINGS_NOT_SAVED: &str = "Couldn't save the settings";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn edit_settings<R>(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Settings) -> R,
    ) -> R {
        let mut next = Settings::clone(&self.runtime.settings);
        let edited = edit(&mut next);
        let sent = self
            .runtime
            .view
            .update(cx, |model, _| model.settings.send(next));
        if let Some(sent) = sent {
            self.runtime.settings = Rc::clone(&sent);
            let settings = Box::new(Settings::clone(&sent));
            self.runtime
                .client
                .post(Command::Settings(SettingsCommand::Save { settings }));
            self.redetect_after_acp_change(cx);
        }
        cx.notify();
        edited
    }

    pub(in crate::environment_shell) fn preview_settings(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Settings),
    ) {
        let mut next = Settings::clone(&self.runtime.settings);
        edit(&mut next);
        self.runtime.settings = self
            .runtime
            .view
            .update(cx, |model, _| model.settings.preview(next));
    }

    pub(in crate::environment_shell) fn settings_adopted(&mut self, cx: &mut Context<Self>) {
        self.apply_active_theme(cx);
    }

    pub(in crate::environment_shell) fn settings_not_saved(&mut self, error: &str) {
        self.push_toast(ToastKind::Error, format!("{SETTINGS_NOT_SAVED}: {error}"));
    }
}
