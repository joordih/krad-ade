use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, App, AppContext as _, Context, Entity, InteractiveElement,
    IntoElement, ParentElement, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use krad_domain::{Hue, TintOpacity, WindowMaterial, WindowTint};

use super::super::dialog_parts::{segment, SegmentSize};
use super::{form_row, segmented_track, EnvironmentShell};
use crate::components::{icon, IconName};
use crate::theme::{
    appearance, apply_window_material, apply_window_tint, hue_rgb, palette, tinted,
    window_material, Appearance, Palette, ICON_SM, RADIUS_SM, SPACE_1, TEXT_XS,
};
use crate::window_frame::{
    offered_materials, replace_window_background, tint_adjustable, usable_material,
    usable_window_background,
};

const MATERIAL_LABEL: &str = "Window material";
const MATERIAL_HINT: &str = "The whole window lets the desktop show through, terminals, code and menus included. Blurred on macOS, on Windows and on KDE under Wayland. Mica needs Windows 11 and Glass, Liquid Glass, needs macOS 26.";
const TINT_COLOR_LABEL: &str = "Tint color";
const TINT_COLOR_HINT: &str =
    "The color laid over the desktop behind the window; it stays translucent.";
const TINT_OPACITY_LABEL: &str = "Tint opacity";
const TINT_OPACITY_HINT: &str = "How much of the desktop the tint covers.";
const TINT_DEFAULT_ID: &str = "window-tint-default";
const TINT_HUE_PREFIX: &str = "window-tint";
const OPACITY_SUFFIX: &str = "%";
const OPACITY_STEP: f32 = 1.0;
const DEFAULT_OPACITY: u8 = 65;
const SLIDER_W: f32 = 180.0;
const OPACITY_LABEL_W: f32 = 36.0;
const SWATCH: f32 = 22.0;
const SWATCH_DOT: f32 = 14.0;
const SWATCH_RING: f32 = 2.0;
const SWATCH_ALPHA: u32 = 0xb3;
const MATERIAL_PREFIX: &str = "window-material";

const fn material_label(material: WindowMaterial) -> &'static str {
    match material {
        WindowMaterial::Opaque => "Opaque",
        WindowMaterial::Blurred => "Blurred",
        WindowMaterial::Mica => "Mica",
        WindowMaterial::Glass => "Glass",
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn sync_window_background(
        &mut self,
        window: &mut Window,
        cx: &App,
    ) {
        let dark = appearance(cx) == Appearance::Dark;
        let wanted = usable_window_background(window_material(cx), dark);
        if self.window_background != Some((wanted, dark)) {
            replace_window_background(window, wanted, dark);
            self.window_background = Some((wanted, dark));
        }
    }

    pub fn follow_system_material(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.material_watch = Some(cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                return;
            }
            let material = this.settings.window_material;
            if usable_material(material) == window_material(cx) {
                return;
            }
            apply_window_material(material, cx);
            this.sync_window_background(window, cx);
            cx.notify();
        }));
    }

    pub(super) fn material_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.window_material;
        let shown = if offered_materials().contains(&current) {
            current
        } else {
            WindowMaterial::Blurred
        };
        let segments = offered_materials().iter().map(|&material| {
            segment(
                (MATERIAL_PREFIX, material as u64),
                material_label(material),
                None,
                shown == material,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.set_window_material(material, window, cx);
            }))
        });
        form_row(
            MATERIAL_LABEL,
            Some(MATERIAL_HINT),
            segmented_track(pal).children(segments).into_any_element(),
            pal,
        )
    }

    fn set_window_material(
        &mut self,
        material: WindowMaterial,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.window_material = material;
        apply_window_material(material, cx);
        self.sync_window_background(window, cx);
        self.save_settings(cx);
    }

    pub(super) fn tint_ui(&self, cx: &mut Context<Self>) -> TintUi {
        let start = self
            .settings
            .window_tint
            .opacity
            .unwrap_or(TintOpacity::new(DEFAULT_OPACITY));
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(f32::from(TintOpacity::MAX))
                .step(OPACITY_STEP)
                .default_value(f32::from(start.percent()))
        });
        let events = cx.subscribe(&opacity, |this, _, event: &SliderEvent, cx| {
            let SliderEvent::Change(value) = event else {
                return;
            };
            let percent = value.start().round();
            let opacity = TintOpacity::new(
                (0..=TintOpacity::MAX)
                    .find(|step| f32::from(*step) >= percent)
                    .unwrap_or(TintOpacity::MAX),
            );
            this.edit_tint(|tint| tint.opacity = Some(opacity), cx);
        });
        TintUi {
            opacity,
            _events: events,
        }
    }

    pub(super) fn tint_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(ui) = &self.settings_ui else {
            return Vec::new();
        };
        if !tint_adjustable(usable_material(self.settings.window_material)) {
            return Vec::new();
        }
        let pal = palette(cx);
        let percent = self
            .settings
            .window_tint
            .opacity
            .unwrap_or(TintOpacity::new(DEFAULT_OPACITY))
            .percent();
        let slider = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1))
            .child(div().w(px(SLIDER_W)).child(Slider::new(&ui.tint.opacity)))
            .child(
                div()
                    .w(px(OPACITY_LABEL_W))
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(format!("{percent}{OPACITY_SUFFIX}")),
            )
            .into_any_element();
        vec![
            form_row(
                TINT_COLOR_LABEL,
                Some(TINT_COLOR_HINT),
                self.tint_swatches(pal, cx),
                pal,
            ),
            form_row(TINT_OPACITY_LABEL, Some(TINT_OPACITY_HINT), slider, pal),
        ]
    }

    fn tint_swatches(&self, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let picked = self.settings.window_tint.hue;
        let cell = |selected: bool| {
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(SWATCH))
                .rounded(px(RADIUS_SM))
                .cursor_pointer()
                .border(px(SWATCH_RING))
                .when(selected, |cell| cell.border_color(rgb(pal.accent)))
                .when(!selected, |cell| {
                    cell.border_color(gpui_kit::transparent_black())
                })
                .hover(|style| style.bg(pal.bg_hover))
        };
        let default = cell(picked.is_none())
            .id(TINT_DEFAULT_ID)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.edit_tint(|tint| tint.hue = None, cx);
            }))
            .child(icon(IconName::Close, ICON_SM, pal.text_muted));
        let hues = Hue::ALL.iter().enumerate().map(|(index, &hue)| {
            cell(picked == Some(hue))
                .id((TINT_HUE_PREFIX, index))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.edit_tint(|tint| tint.hue = Some(hue), cx);
                }))
                .child(
                    div()
                        .size(px(SWATCH_DOT))
                        .rounded_full()
                        .bg(tinted(hue_rgb(hue, pal), SWATCH_ALPHA)),
                )
                .into_any_element()
        });
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_1))
            .child(default)
            .children(hues)
            .into_any_element()
    }

    fn edit_tint(&mut self, edit: impl FnOnce(&mut WindowTint), cx: &mut Context<Self>) {
        edit(&mut self.settings.window_tint);
        apply_window_tint(self.settings.window_tint, cx);
        self.save_settings(cx);
        cx.notify();
    }
}

pub(super) struct TintUi {
    opacity: Entity<SliderState>,
    _events: Subscription,
}
