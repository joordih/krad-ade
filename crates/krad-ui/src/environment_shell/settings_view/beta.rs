use gpui_kit::{AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement};
use krad_domain::ComposerDesign;

use super::super::dialog_parts::{segment, SegmentSize};
use super::{form_row, panel_card, segmented_track, EnvironmentShell};
use crate::theme::palette;

const COMPOSER_PREFIX: &str = "beta-composer";
const COMPOSER_LABEL: &str = "Composer (beta)";
const COMPOSER_HINT: &str = "Try the redesigned chat composer and model picker. Every feature of the current one stays; switch back at any time.";

impl EnvironmentShell {
    pub(super) fn beta_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.beta.composer;
        let segments = ComposerDesign::ALL.map(|design| {
            segment(
                (COMPOSER_PREFIX, design as u64),
                design.display_name(),
                None,
                current == design,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.settings.beta.composer = design;
                this.save_settings(cx);
                cx.notify();
            }))
        });
        let control = segmented_track(pal).children(segments).into_any_element();
        panel_card(
            vec![form_row(COMPOSER_LABEL, Some(COMPOSER_HINT), control, pal)],
            pal,
        )
    }
}
