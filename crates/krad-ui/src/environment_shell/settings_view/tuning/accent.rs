use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Sizable as _, Size};
use gpui_kit::{
    div, px, AnyElement, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use krad_domain::{AccentChoice, Hue, ThemeColor, ThemeTuning};

use super::super::form_row;
use super::super::material::{swatch_cell, swatch_dot};
use super::{hex_field, reset_button, tuning_section, typed_color, TuningUi, TuningWrite};
use crate::components::{icon, IconName};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{hue_rgb, Palette, ICON_SM, SPACE_1, SPACE_2};

const SECTION: &str = "Accent color";
const SECTION_HINT: &str =
    "Colors selections, focus rings, primary buttons and the highlighted menu row.";
const SWATCHES_LABEL: &str = "Accent";
const SWATCHES_HINT: &str = "The theme's own accent, a swatch, or your color.";
const CUSTOM_LABEL: &str = "Custom color";
const CUSTOM_HINT: &str =
    "A #rrggbb color. Labels on it switch to black or white to stay readable.";
const PLACEHOLDER: &str = "#rrggbb";
const THEME_ID: &str = "accent-theme";
const SWATCH_PREFIX: &str = "accent-swatch";
const CUSTOM_ID: &str = "accent-custom";
const RESET_ID: &str = "accent-reset";
const FIELD_W: f32 = 120.0;

impl EnvironmentShell {
    pub(super) fn accent_field(
        tuning: &ThemeTuning,
        events: &mut Vec<Subscription>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let custom = match tuning.accent {
            AccentChoice::Custom(color) => Some(color),
            AccentChoice::Theme | AccentChoice::Swatch(_) => None,
        };
        let field = hex_field(custom, PLACEHOLDER, window, cx);
        events.push(
            cx.subscribe(&field, |this: &mut Self, field, event: &InputEvent, cx| {
                let Some(text) = typed_color(&field, event, cx) else {
                    return;
                };
                let Ok(color) = text.parse::<ThemeColor>() else {
                    return;
                };
                this.edit_tuning(
                    |tuning| tuning.accent = AccentChoice::Custom(color),
                    TuningWrite::Save,
                    cx,
                );
            }),
        );
        field
    }

    fn pick_accent(&mut self, choice: AccentChoice, cx: &mut Context<Self>) {
        self.edit_tuning(|tuning| tuning.accent = choice, TuningWrite::Save, cx);
    }

    fn reset_accent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_accent(AccentChoice::Theme, cx);
        if let Some(ui) = &self.settings_ui {
            ui.tuning
                .accent
                .update(cx, |field, cx| field.set_value("", window, cx));
        }
    }

    fn accent_swatches(&self, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let picked = self.settings.theme_tuning.accent;
        let theme = swatch_cell(picked == AccentChoice::Theme, pal)
            .id(THEME_ID)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.pick_accent(AccentChoice::Theme, cx);
            }))
            .child(icon(IconName::Close, ICON_SM, pal.text_muted));
        let hues = Hue::ALL.iter().enumerate().map(|(index, &hue)| {
            swatch_cell(picked == AccentChoice::Swatch(hue), pal)
                .id((SWATCH_PREFIX, index))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.pick_accent(AccentChoice::Swatch(hue), cx);
                }))
                .child(swatch_dot(hue_rgb(hue, pal)))
                .into_any_element()
        });
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_1))
            .child(theme)
            .children(hues)
            .into_any_element()
    }

    fn accent_custom(&self, ui: &TuningUi, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let picked = self.settings.theme_tuning.accent;
        let custom = match picked {
            AccentChoice::Custom(color) => Some(color),
            AccentChoice::Theme | AccentChoice::Swatch(_) => None,
        };
        let typed = ui.accent.read(cx).value().trim().parse::<ThemeColor>().ok();
        let shown = custom.or(typed);
        let cell = swatch_cell(custom.is_some(), pal).id(CUSTOM_ID);
        let cell = match shown {
            Some(color) => cell
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.pick_accent(AccentChoice::Custom(color), cx);
                }))
                .child(swatch_dot(color.0)),
            None => cell.child(icon(IconName::Add, ICON_SM, pal.text_muted)),
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(cell)
            .child(
                div()
                    .w(px(FIELD_W))
                    .child(Input::new(&ui.accent).with_size(Size::Small)),
            )
            .into_any_element()
    }

    pub(super) fn accent_section(
        &self,
        ui: &TuningUi,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = [
            form_row(
                SWATCHES_LABEL,
                Some(SWATCHES_HINT),
                self.accent_swatches(pal, cx),
                pal,
            ),
            form_row(
                CUSTOM_LABEL,
                Some(CUSTOM_HINT),
                self.accent_custom(ui, pal, cx),
                pal,
            ),
        ];
        let reset =
            reset_button(RESET_ID).on_click(cx.listener(|this: &mut Self, _, window, cx| {
                this.reset_accent(window, cx);
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
