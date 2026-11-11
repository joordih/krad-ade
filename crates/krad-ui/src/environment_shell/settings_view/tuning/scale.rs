use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement as _, Styled as _,
};
use krad_domain::InterfaceScale;

use super::super::super::dialog_parts::{segment, SegmentSize};
use super::super::{form_row, segmented_track};
use super::{reset_button, tuning_section, TuningWrite};
use crate::environment_shell::EnvironmentShell;
use crate::theme::Palette;

const SECTION: &str = "Interface scale";
const SECTION_HINT: &str = "Scales the interface. Terminal and editor font sizes stay as they are.";
const SCALE_LABEL: &str = "Scale";
const SCALE_HINT: &str = "Larger sizes suit high-resolution displays and reading from afar.";
const SCALE_PREFIX: &str = "interface-scale";
const RESET_ID: &str = "interface-scale-reset";
const PERCENT_SUFFIX: &str = "%";

impl EnvironmentShell {
    fn set_interface_scale(&mut self, scale: InterfaceScale, cx: &mut Context<Self>) {
        self.edit_tuning(
            |tuning| tuning.interface_scale = scale,
            TuningWrite::Save,
            cx,
        );
    }

    pub(super) fn scale_section(&self, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let current = self.settings.theme_tuning.interface_scale;
        let segments = InterfaceScale::steps().map(|scale| {
            segment(
                (SCALE_PREFIX, u64::from(scale.percent())),
                format!("{}{PERCENT_SUFFIX}", scale.percent()),
                None,
                current == scale,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.set_interface_scale(scale, cx);
            }))
        });
        let row = form_row(
            SCALE_LABEL,
            Some(SCALE_HINT),
            segmented_track(pal)
                .flex_wrap()
                .children(segments)
                .into_any_element(),
            pal,
        );
        let reset = reset_button(RESET_ID).on_click(cx.listener(|this: &mut Self, _, _, cx| {
            this.set_interface_scale(InterfaceScale::default(), cx);
        }));
        tuning_section(
            SECTION,
            SECTION_HINT,
            Some(reset.into_any_element()),
            [row],
            pal,
        )
    }
}
