use gpui_kit::{Context, PromptButton, PromptLevel, Window};

use super::EnvironmentShell;

const NATIVE_ALERTS: bool = cfg!(target_os = "macos");
const CONFIRMED: usize = 0;

pub(super) struct Confirmation<'a> {
    pub(super) title: &'static str,
    pub(super) detail: &'a str,
    pub(super) confirm: &'static str,
    pub(super) cancel: &'static str,
}

impl EnvironmentShell {
    pub(super) fn confirm_natively(
        confirmation: &Confirmation<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
        answered: impl FnOnce(&mut Self, bool, &mut Window, &mut Context<Self>) + 'static,
    ) -> bool {
        if !NATIVE_ALERTS {
            return false;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            confirmation.title,
            Some(confirmation.detail),
            &[
                PromptButton::ok(confirmation.confirm),
                PromptButton::cancel(confirmation.cancel),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let confirmed = matches!(answer.await, Ok(CONFIRMED));
            this.update_in(cx, |this, window, cx| answered(this, confirmed, window, cx))
                .unwrap_or_default();
        })
        .detach();
        true
    }
}
