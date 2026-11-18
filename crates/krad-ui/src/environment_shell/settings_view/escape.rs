use gpui_kit::{Context, Window};

use super::super::EnvironmentShell;
use super::gitlab::GitlabStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsEscape {
    Ignore,
    Unfocus,
    Close,
}

const fn settings_escape(sheet_open: bool, child_focused: bool) -> SettingsEscape {
    if sheet_open {
        SettingsEscape::Ignore
    } else if child_focused {
        SettingsEscape::Unfocus
    } else {
        SettingsEscape::Close
    }
}

impl EnvironmentShell {
    fn settings_sheet_open(&self) -> bool {
        self.settings_ui.as_ref().is_some_and(|ui| {
            ui.theme_editor.is_some()
                || ui.cloud_runners.sign_in.is_some()
                || matches!(ui.gitlab.status, GitlabStatus::WaitingForCode(_))
        })
    }

    pub(super) fn escape_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(focus) = self.settings_ui.as_ref().map(|ui| ui.focus.clone()) else {
            return;
        };
        let child_focused = focus.contains_focused(window, cx) && !focus.is_focused(window);
        match settings_escape(self.settings_sheet_open(), child_focused) {
            SettingsEscape::Ignore => {}
            SettingsEscape::Unfocus => window.focus(&focus, cx),
            SettingsEscape::Close => self.close_settings(window, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_leaves_settings_only_once_nothing_inside_takes_it() {
        assert_eq!(settings_escape(false, false), SettingsEscape::Close);
        assert_eq!(settings_escape(false, true), SettingsEscape::Unfocus);
        assert_eq!(settings_escape(true, false), SettingsEscape::Ignore);
        assert_eq!(settings_escape(true, true), SettingsEscape::Ignore);
    }
}
