use gpui_kit::{AnyElement, Context, Window};

use super::super::super::host::{SignInDialog, SignInPlace};
use super::super::{EnvironmentShell, SettingsPane};

impl EnvironmentShell {
    pub(in crate::environment_shell) fn runner_sign_in_dialog(
        &mut self,
    ) -> Option<&mut SignInDialog> {
        self.settings_ui.as_mut()?.cloud_runners.sign_in.as_mut()
    }

    pub(in crate::environment_shell) fn close_runner_sign_in(&mut self) {
        if let Some(ui) = &mut self.settings_ui {
            ui.cloud_runners.sign_in = None;
        }
    }

    pub(in crate::environment_shell) fn runner_sign_in_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.settings_ui.as_ref()?.cloud_runners.sign_in.as_ref()?;
        self.sign_in_sheet(dialog, SignInPlace::Settings, cx)
    }

    pub(in crate::environment_shell) fn show_runner_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let showing = self
            .settings_ui
            .as_ref()
            .is_some_and(|ui| ui.cloud_runners.sign_in.is_some());
        if showing || !self.runtime.view.read(cx).hosts.runner_prompt_waits() {
            return;
        }
        if self.settings_ui.is_none() {
            cx.defer_in(window, |this, window, cx| {
                if this.settings_ui.is_none() {
                    this.open_settings(window, cx);
                }
                this.select_pane(SettingsPane::CloudRunners, cx);
            });
            return;
        }
        let Some(prompt) = self
            .runtime
            .view
            .update(cx, |model, _| model.hosts.next_runner_prompt())
        else {
            return;
        };
        let dialog = self.sign_in_dialog(prompt, SignInPlace::Settings, window, cx);
        let Some(ui) = &mut self.settings_ui else {
            return;
        };
        ui.cloud_runners.sign_in = Some(dialog);
        if ui.pane == SettingsPane::CloudRunners {
            cx.notify();
        } else {
            self.select_pane(SettingsPane::CloudRunners, cx);
        }
    }
}
