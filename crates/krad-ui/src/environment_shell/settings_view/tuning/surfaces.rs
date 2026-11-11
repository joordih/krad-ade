use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use krad_domain::{DarkLightness, HueDegrees, MainPaneMaterial, Percent, SurfaceTone, ThemeTuning};

use super::super::form_row;
use super::super::material::{slider_control, slider_percent, swatch_dot};
use super::{reset_button, tuning_section, TuningUi, TuningWrite};
use crate::components::toggle_switch;
use crate::environment_shell::EnvironmentShell;
use crate::theme::{hsl_rgb, Palette, SPACE_1};

const SECTION: &str = "Surfaces";
const SECTION_HINT: &str = "The hue under the window and the main panes, and how much glass shows.";
const HUE_LABEL: &str = "Hue";
const HUE_HINT: &str = "The base hue of the window and main-pane surfaces.";
const SATURATION_LABEL: &str = "Saturation";
const SATURATION_HINT: &str = "How much of that hue tints the surfaces. 0% keeps them neutral.";
const LIGHTNESS_LABEL: &str = "Dark mode lightness";
const LIGHTNESS_HINT: &str =
    "How light the dark surfaces are. Text brightens or the surface darkens to stay readable.";
const SIDEBAR_LABEL: &str = "Sidebar opacity";
const SIDEBAR_HINT: &str = "How solid the sidebar is over the window's glass.";
const PANE_GLASS_LABEL: &str = "Main pane glass";
const PANE_GLASS_HINT: &str =
    "Let the window's glass show through the main panes, or keep them opaque.";
const PANE_GLASS_ID: &str = "main-pane-glass";
const PANE_GLASS_KNOB: &str = "main-pane-glass-knob";
const RESET_ID: &str = "surfaces-reset";
const DEGREES_SUFFIX: &str = "°";
const PERCENT_SUFFIX: &str = "%";
const HUE_STEP: f32 = 5.0;
const PERCENT_STEP: f32 = 1.0;
const PREVIEW_SATURATION: f32 = 0.7;
const PREVIEW_LIGHTNESS: f32 = 0.5;
const WHOLE: f32 = 100.0;

pub(super) struct SurfaceSliders {
    hue: Entity<SliderState>,
    saturation: Entity<SliderState>,
    lightness: Entity<SliderState>,
    sidebar: Entity<SliderState>,
}

#[derive(Clone, Copy)]
enum SurfaceSlider {
    Hue,
    Saturation,
    Lightness,
    Sidebar,
}

impl SurfaceSlider {
    fn range(self) -> (f32, f32, f32) {
        match self {
            Self::Hue => (0.0, f32::from(HueDegrees::MAX), HUE_STEP),
            Self::Saturation | Self::Sidebar => (0.0, f32::from(Percent::MAX), PERCENT_STEP),
            Self::Lightness => (
                f32::from(DarkLightness::MIN),
                f32::from(DarkLightness::MAX),
                PERCENT_STEP,
            ),
        }
    }

    fn value(self, tuning: &ThemeTuning) -> f32 {
        let surfaces = tuning.surfaces;
        match self {
            Self::Hue => f32::from(surfaces.hue.value()),
            Self::Saturation => f32::from(surfaces.saturation.value()),
            Self::Lightness => f32::from(surfaces.dark_lightness.value()),
            Self::Sidebar => f32::from(tuning.sidebar_opacity.unwrap_or_default().value()),
        }
    }

    fn apply(self, value: f32, tuning: &mut ThemeTuning) {
        match self {
            Self::Hue => {
                tuning.surfaces.hue = HueDegrees::new(slider_percent(value, HueDegrees::MAX));
            }
            Self::Saturation => {
                tuning.surfaces.saturation = Percent::new(slider_percent(value, Percent::MAX));
            }
            Self::Lightness => {
                tuning.surfaces.dark_lightness =
                    DarkLightness::new(slider_percent(value, DarkLightness::MAX));
            }
            Self::Sidebar => {
                tuning.sidebar_opacity = Some(Percent::new(slider_percent(value, Percent::MAX)));
            }
        }
    }
}

impl EnvironmentShell {
    fn surface_slider(
        slider: SurfaceSlider,
        tuning: &ThemeTuning,
        events: &mut Vec<Subscription>,
        cx: &mut Context<Self>,
    ) -> Entity<SliderState> {
        let (min, max, step) = slider.range();
        let start = slider.value(tuning);
        let state = cx.new(|_| {
            SliderState::new()
                .min(min)
                .max(max)
                .step(step)
                .default_value(start)
        });
        events.push(
            cx.subscribe(&state, move |this, _, event: &SliderEvent, cx| {
                let (value, write) = match event {
                    SliderEvent::Change(value) => (value.start(), TuningWrite::Preview),
                    SliderEvent::Release(value) => (value.start(), TuningWrite::Save),
                };
                this.edit_tuning(|tuning| slider.apply(value, tuning), write, cx);
            }),
        );
        state
    }

    pub(super) fn surface_sliders(
        tuning: &ThemeTuning,
        events: &mut Vec<Subscription>,
        cx: &mut Context<Self>,
    ) -> SurfaceSliders {
        SurfaceSliders {
            hue: Self::surface_slider(SurfaceSlider::Hue, tuning, events, cx),
            saturation: Self::surface_slider(SurfaceSlider::Saturation, tuning, events, cx),
            lightness: Self::surface_slider(SurfaceSlider::Lightness, tuning, events, cx),
            sidebar: Self::surface_slider(SurfaceSlider::Sidebar, tuning, events, cx),
        }
    }

    fn reset_surfaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_tuning(
            |tuning| {
                tuning.surfaces = SurfaceTone::default();
                tuning.sidebar_opacity = None;
                tuning.main_pane = MainPaneMaterial::default();
            },
            TuningWrite::Save,
            cx,
        );
        let tuning = self.settings.theme_tuning;
        let Some(ui) = &self.settings_ui else {
            return;
        };
        let sliders = &ui.tuning.surfaces;
        for (state, slider) in [
            (&sliders.hue, SurfaceSlider::Hue),
            (&sliders.saturation, SurfaceSlider::Saturation),
            (&sliders.lightness, SurfaceSlider::Lightness),
            (&sliders.sidebar, SurfaceSlider::Sidebar),
        ] {
            let value = slider.value(&tuning);
            state.update(cx, |state, cx| state.set_value(value, window, cx));
        }
    }

    fn pane_glass_switch(&self, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let glass = self.settings.theme_tuning.main_pane.is_glass();
        div()
            .id(PANE_GLASS_ID)
            .cursor_pointer()
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                let next = if glass {
                    MainPaneMaterial::Opaque
                } else {
                    MainPaneMaterial::Glass
                };
                this.edit_tuning(|tuning| tuning.main_pane = next, TuningWrite::Save, cx);
            }))
            .child(toggle_switch(PANE_GLASS_KNOB, glass, pal))
            .into_any_element()
    }

    pub(super) fn surfaces_section(
        &self,
        ui: &TuningUi,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tuning = self.settings.theme_tuning;
        let surfaces = tuning.surfaces;
        let hue = surfaces.hue.value();
        let preview = hsl_rgb(f32::from(hue), PREVIEW_SATURATION, PREVIEW_LIGHTNESS);
        let tone = hsl_rgb(
            f32::from(hue),
            surfaces.saturation.fraction(),
            f32::from(surfaces.dark_lightness.value()) / WHOLE,
        );
        let hue_control = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1))
            .child(swatch_dot(preview))
            .child(slider_control(
                &ui.surfaces.hue,
                format!("{hue}{DEGREES_SUFFIX}"),
                pal,
            ))
            .into_any_element();
        let saturation_control = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1))
            .child(swatch_dot(tone))
            .child(slider_control(
                &ui.surfaces.saturation,
                format!("{}{PERCENT_SUFFIX}", surfaces.saturation.value()),
                pal,
            ))
            .into_any_element();
        let sidebar = tuning.sidebar_opacity.unwrap_or_default().value();
        let rows = [
            form_row(HUE_LABEL, Some(HUE_HINT), hue_control, pal),
            form_row(
                SATURATION_LABEL,
                Some(SATURATION_HINT),
                saturation_control,
                pal,
            ),
            form_row(
                LIGHTNESS_LABEL,
                Some(LIGHTNESS_HINT),
                slider_control(
                    &ui.surfaces.lightness,
                    format!("{}{PERCENT_SUFFIX}", surfaces.dark_lightness.value()),
                    pal,
                ),
                pal,
            ),
            form_row(
                SIDEBAR_LABEL,
                Some(SIDEBAR_HINT),
                slider_control(
                    &ui.surfaces.sidebar,
                    format!("{sidebar}{PERCENT_SUFFIX}"),
                    pal,
                ),
                pal,
            ),
            form_row(
                PANE_GLASS_LABEL,
                Some(PANE_GLASS_HINT),
                self.pane_glass_switch(pal, cx),
                pal,
            ),
        ];
        let reset =
            reset_button(RESET_ID).on_click(cx.listener(|this: &mut Self, _, window, cx| {
                this.reset_surfaces(window, cx);
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
