use gpui_kit::{
    div, px, AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
};
use krad_domain::CopyMode;

use super::super::dialog_parts::{segment, SegmentSize};
use super::{form_row, panel_card, segmented_track, EnvironmentShell, SWATCH_GAP};
use crate::keymap::{COPY_HINT, PASTE_HINT};
use crate::theme::palette;

const COPY_MODE_PREFIX: &str = "copy-mode";
const COPY_LABEL: &str = "Copy";

impl EnvironmentShell {
    pub(super) fn terminal_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(SWATCH_GAP))
            .child(self.copy_mode_card(cx))
            .child(self.font_card(super::FontZone::Terminal, cx))
            .into_any_element()
    }

    fn copy_mode_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.copy_mode;
        let options = CopyMode::all().map(|mode| {
            segment(
                (COPY_MODE_PREFIX, mode as u64),
                mode.display_name(),
                None,
                current == mode,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.settings.copy_mode = mode;
                this.save_settings(cx);
                cx.notify();
            }))
        });
        let hint =
            format!("Copy with {COPY_HINT}, by selecting text, or both. Paste with {PASTE_HINT}.");
        panel_card(
            [(form_row(
                COPY_LABEL,
                Some(&hint),
                segmented_track(pal).children(options).into_any_element(),
                pal,
            ))
            .into_any_element()],
            pal,
        )
        .into_any_element()
    }
}
