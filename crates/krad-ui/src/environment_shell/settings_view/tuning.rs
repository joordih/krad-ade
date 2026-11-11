mod accent;
mod diff;
mod scale;
mod surfaces;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement, Styled,
    Subscription, Window,
};
use krad_domain::{ThemeColor, ThemeTuning};

use super::gitlab::page::section_heading;
use super::sections::HEADING_GAP;
use super::{panel_card, EnvironmentShell};
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{palette, Palette, SPACE_2};

const RESET_LABEL: &str = "Reset";

#[derive(Clone, Copy, PartialEq, Eq)]
enum TuningWrite {
    Preview,
    Save,
}

pub(super) struct TuningUi {
    surfaces: surfaces::SurfaceSliders,
    accent: Entity<InputState>,
    diff: diff::DiffFields,
    _events: Vec<Subscription>,
}

pub(super) fn reset_button(id: &'static str) -> Button {
    Button::new(id)
        .label(RESET_LABEL)
        .icon(IconName::Undo)
        .variant(ButtonVariant::Ghost)
        .small()
}

pub(super) fn tuning_section(
    title: &str,
    detail: &str,
    action: Option<AnyElement>,
    rows: impl IntoIterator<Item = AnyElement>,
    pal: &Palette,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(HEADING_GAP))
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .gap(px(SPACE_2))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(section_heading(title, Some(detail), pal)),
                )
                .children(action),
        )
        .child(panel_card(rows, pal))
        .into_any_element()
}

fn hex_text(color: Option<ThemeColor>) -> String {
    color.map_or_else(String::new, |color| color.to_string())
}

fn hex_field(
    color: Option<ThemeColor>,
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<EnvironmentShell>,
) -> Entity<InputState> {
    let value = hex_text(color);
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .default_value(value)
    })
}

fn typed_color(
    field: &Entity<InputState>,
    event: &InputEvent,
    cx: &gpui_kit::App,
) -> Option<String> {
    matches!(event, InputEvent::Change).then(|| field.read(cx).value().trim().to_owned())
}

impl EnvironmentShell {
    pub(super) fn tuning_ui(&self, window: &mut Window, cx: &mut Context<Self>) -> TuningUi {
        let tuning = self.settings.theme_tuning;
        let mut events = Vec::new();
        let surfaces = Self::surface_sliders(&tuning, &mut events, cx);
        let accent = Self::accent_field(&tuning, &mut events, window, cx);
        let diff = Self::diff_fields(&tuning, &mut events, window, cx);
        TuningUi {
            surfaces,
            accent,
            diff,
            _events: events,
        }
    }

    pub(super) fn tuning_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(ui) = &self.settings_ui else {
            return Vec::new();
        };
        let pal = palette(cx);
        vec![
            self.accent_section(&ui.tuning, pal, cx),
            self.surfaces_section(&ui.tuning, pal, cx),
            self.diff_section(&ui.tuning, pal, cx),
            self.scale_section(pal, cx),
        ]
    }

    fn edit_tuning(
        &mut self,
        edit: impl FnOnce(&mut ThemeTuning),
        write: TuningWrite,
        cx: &mut Context<Self>,
    ) {
        let before = self.settings.theme_tuning;
        edit(&mut self.settings.theme_tuning);
        if self.settings.theme_tuning != before {
            self.apply_active_theme(cx);
        }
        if write == TuningWrite::Save {
            self.save_settings(cx);
        }
        cx.notify();
    }
}
