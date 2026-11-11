use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Sizable as _, Size};
use gpui_kit::{
    div, px, AnyElement, Context, Entity, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use krad_domain::{DiffColorSlot, DiffColors, DiffPreset, ThemeColor, ThemeTuning};

use super::super::super::dialog_parts::{segment, SegmentSize};
use super::super::material::swatch_dot;
use super::super::{form_row, segmented_track};
use super::{hex_field, reset_button, tuning_section, typed_color, TuningUi, TuningWrite};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{Palette, SPACE_2};

const SECTION: &str = "Diff colors";
const SECTION_HINT: &str =
    "Added, removed and modified lines in reviews, the changes tab and the file tree.";
const PRESET_LABEL: &str = "Preset";
const PRESET_HINT: &str = "Red and blue reads well with red-green color blindness.";
const SLOT_HINT: &str = "Leave empty to use the preset's color.";
const PLACEHOLDER: &str = "Preset color";
const PRESET_PREFIX: &str = "diff-preset";
const RESET_ID: &str = "diff-reset";
const FIELD_W: f32 = 120.0;

pub(super) struct DiffFields([(DiffColorSlot, Entity<InputState>); 3]);

const fn slot_color(slot: DiffColorSlot, pal: &Palette) -> u32 {
    match slot {
        DiffColorSlot::Added => pal.diff_added,
        DiffColorSlot::Removed => pal.diff_removed,
        DiffColorSlot::Modified => pal.diff_modified,
    }
}

const fn preset_key(preset: DiffPreset) -> u64 {
    match preset {
        DiffPreset::Default => 0,
        DiffPreset::ColorBlind => 1,
        DiffPreset::HighContrast => 2,
    }
}

impl EnvironmentShell {
    pub(super) fn diff_fields(
        tuning: &ThemeTuning,
        events: &mut Vec<Subscription>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DiffFields {
        DiffFields(DiffColorSlot::ALL.map(|slot| {
            let field = hex_field(tuning.diff.custom(slot), PLACEHOLDER, window, cx);
            events.push(cx.subscribe(
                &field,
                move |this: &mut Self, field, event: &InputEvent, cx| {
                    let Some(text) = typed_color(&field, event, cx) else {
                        return;
                    };
                    let color = if text.is_empty() {
                        None
                    } else {
                        let Ok(color) = text.parse::<ThemeColor>() else {
                            return;
                        };
                        Some(color)
                    };
                    this.edit_tuning(
                        |tuning| tuning.diff.set_custom(slot, color),
                        TuningWrite::Save,
                        cx,
                    );
                },
            ));
            (slot, field)
        }))
    }

    fn reset_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_tuning(
            |tuning| tuning.diff = DiffColors::default(),
            TuningWrite::Save,
            cx,
        );
        let Some(ui) = &self.settings_ui else {
            return;
        };
        for (_, field) in &ui.tuning.diff.0 {
            field.update(cx, |field, cx| field.set_value("", window, cx));
        }
    }

    fn diff_presets(&self, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let current = self.settings.theme_tuning.diff.preset;
        let segments = DiffPreset::ALL.iter().map(|&preset| {
            segment(
                (PRESET_PREFIX, preset_key(preset)),
                preset.display_name(),
                None,
                current == preset,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_tuning(|tuning| tuning.diff.preset = preset, TuningWrite::Save, cx);
            }))
        });
        segmented_track(pal).children(segments).into_any_element()
    }

    pub(super) fn diff_section(
        &self,
        ui: &TuningUi,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let preset = form_row(
            PRESET_LABEL,
            Some(PRESET_HINT),
            self.diff_presets(pal, cx),
            pal,
        );
        let slots = ui.diff.0.iter().map(|(slot, field)| {
            let control = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_2))
                .child(swatch_dot(slot_color(*slot, pal)))
                .child(
                    div()
                        .w(px(FIELD_W))
                        .child(Input::new(field).with_size(Size::Small)),
                )
                .into_any_element();
            form_row(slot.display_name(), Some(SLOT_HINT), control, pal)
        });
        let rows: Vec<AnyElement> = std::iter::once(preset).chain(slots).collect();
        let reset =
            reset_button(RESET_ID).on_click(cx.listener(|this: &mut Self, _, window, cx| {
                this.reset_diff(window, cx);
            }));
        tuning_section(
            SECTION,
            SECTION_HINT,
            Some(reset.into_any_element()),
            rows,
            pal,
        )
    }
}
