use gpui_kit::{AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement};
use krad_domain::ToolchainActivation;

use super::super::dialog_parts::{segment, SegmentSize};
use super::{form_row, panel_card, segmented_track, EnvironmentShell};
use crate::theme::palette;

const ACTIVATION_LABEL: &str = "Activate in new terminals";
const ACTIVATION_HINT: &str = "New terminals of a group run the activation of the toolchain the editor uses for the group's folder, such as a virtual environment. A group can override this from its menu. Agents are never activated.";
const BTN_ACTIVATION_OFF: &str = "toolchain-activation-off";
const BTN_ACTIVATION_ON: &str = "toolchain-activation-on";
const OFF_LABEL: &str = "Off";
const ON_LABEL: &str = "On";

impl EnvironmentShell {
    fn set_toolchain_activation(
        &mut self,
        activation: ToolchainActivation,
        cx: &mut Context<Self>,
    ) {
        self.settings.toolchain_activation = activation;
        self.save_settings(cx);
        cx.notify();
    }

    pub(super) fn toolchain_activation_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.toolchain_activation;
        let option = |id: &'static str, label: &'static str, activation: ToolchainActivation| {
            segment(
                id,
                label,
                None,
                current == activation,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.set_toolchain_activation(activation, cx);
            }))
        };
        let segmented = segmented_track(pal)
            .child(option(
                BTN_ACTIVATION_OFF,
                OFF_LABEL,
                ToolchainActivation::Off,
            ))
            .child(option(BTN_ACTIVATION_ON, ON_LABEL, ToolchainActivation::On))
            .into_any_element();
        panel_card(
            vec![form_row(
                ACTIVATION_LABEL,
                Some(ACTIVATION_HINT),
                segmented,
                pal,
            )],
            pal,
        )
    }
}
