use gpui_kit::{Context, Window};
use krad_domain::COMPOSER_MIN_ROWS;

use super::super::EnvironmentShell;
use crate::theme::LINE_SM;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn sync_composer_rows(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let window_h = window.viewport_size().height.as_f32();
        let max_rows = self.settings.composer_height.max_rows(window_h, LINE_SM);
        let stale = self
            .envs
            .values_mut()
            .flat_map(|live| live.chats.values_mut())
            .filter(|chat| chat.composer_rows != max_rows);
        for chat in stale {
            chat.composer_rows = max_rows;
            chat.input.update(cx, |input, cx| {
                input.set_auto_grow(usize::from(COMPOSER_MIN_ROWS), usize::from(max_rows), cx);
            });
        }
    }
}
