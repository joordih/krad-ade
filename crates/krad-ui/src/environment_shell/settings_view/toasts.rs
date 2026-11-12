use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
};
use krad_domain::ToastLifetime;

use super::super::dialog_parts::{segment, SegmentSize};
use super::{form_row, panel_card, segmented_track, EnvironmentShell};
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::palette;

pub(super) const TOASTS_TITLE: &str = "Toasts";
pub(super) const TOASTS_DETAIL: &str = "Short messages shown over the window while you use it.";
const DURATION_LABEL: &str = "Duration";
const DURATION_HINT: &str = "Every kind lasts the same; pointing at a toast holds it.";
const PLACE_LABEL: &str = "Position";
const PLACE_HINT: &str = "Drag a sample where toasts should appear. On the left half the icon sits on the right, on the right half on the left.";
const PLACE_BUTTON: &str = "Place…";
const ID_PREFIX: &str = "toast-setting";
const BTN_PLACE: &str = "toast-setting-place";

impl EnvironmentShell {
    pub(super) fn toasts_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.toasts.lifetime;
        let segments = ToastLifetime::ALL.map(|lifetime| {
            segment(
                SharedString::from(format!("{ID_PREFIX}-{}", lifetime.display_name())),
                lifetime.display_name(),
                None,
                current == lifetime,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.settings.toasts.lifetime = lifetime;
                this.save_settings(cx);
                cx.notify();
            }))
        });
        let duration = segmented_track(pal).children(segments).into_any_element();
        let place = Button::new(BTN_PLACE)
            .label(PLACE_BUTTON)
            .icon(IconName::Move)
            .variant(ButtonVariant::Secondary)
            .small()
            .on_click(cx.listener(|this: &mut Self, _, _, cx| this.start_toast_placing(cx)))
            .into_any_element();
        panel_card(
            [
                form_row(DURATION_LABEL, Some(DURATION_HINT), duration, pal),
                form_row(PLACE_LABEL, Some(PLACE_HINT), place, pal),
            ],
            pal,
        )
    }
}
