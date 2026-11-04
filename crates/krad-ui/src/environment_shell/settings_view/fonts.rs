use gpui_kit::{
    div, px, rgb, AnyElement, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Pixels, Point, StatefulInteractiveElement, Styled,
};
use krad_domain::{FontChoice, FontSize, Ligatures, Settings};

use super::super::context_menu::{MenuAction, MenuEntry, MenuSearch};
use super::super::dialog_parts::{segment, SegmentSize};
use super::{form_row, panel_card, segmented_track, EnvironmentShell};
use crate::components::{icon, Button, IconName};
use crate::theme::{
    apply_editor_font, apply_interface_font, palette, EDITOR_FONT_SIZE, ICON_SM, LINE_SM,
    RADIUS_SM, SPACE_1, SPACE_2, SYSTEM_MONO_LABEL, TERMINAL_FONT_SIZE, TEXT_SM,
};

const FAMILY_LABEL: &str = "Font";
const SIZE_LABEL: &str = "Size";
const LIGATURES_LABEL: &str = "Ligatures";
const LIGATURES_HINT: &str =
    "Join sequences such as => or != into one glyph, in fonts that draw them.";
const INTERFACE_HINT: &str =
    "Text fonts installed on this computer; script and symbol fonts are left out.";
const CODE_HINT: &str = "Monospaced fonts installed on this computer.";
const SYSTEM_UI_LABEL: &str = "System";
const FILTER_PLACEHOLDER: &str = "Filter fonts…";
const PICKER_PREFIX: &str = "font-family";
const SMALLER_PREFIX: &str = "font-smaller";
const LARGER_PREFIX: &str = "font-larger";
const LIGATURES_PREFIX: &str = "font-ligatures";
const PICKER_W: f32 = 220.0;
const PICKER_H: f32 = 28.0;
const SIZE_W: f32 = 36.0;
const SIZE_STEP: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::environment_shell) enum FontZone {
    Interface,
    Editor,
    Terminal,
}

impl FontZone {
    const fn choice(self, settings: &Settings) -> &FontChoice {
        match self {
            Self::Interface => &settings.interface_font,
            Self::Editor => &settings.editor_font,
            Self::Terminal => &settings.terminal_font,
        }
    }

    const fn choice_mut(self, settings: &mut Settings) -> &mut FontChoice {
        match self {
            Self::Interface => &mut settings.interface_font,
            Self::Editor => &mut settings.editor_font,
            Self::Terminal => &mut settings.terminal_font,
        }
    }

    const fn default_size(self) -> Option<FontSize> {
        match self {
            Self::Interface => None,
            Self::Editor => Some(EDITOR_FONT_SIZE),
            Self::Terminal => Some(TERMINAL_FONT_SIZE),
        }
    }

    const fn default_label(self) -> &'static str {
        match self {
            Self::Interface => SYSTEM_UI_LABEL,
            Self::Editor | Self::Terminal => SYSTEM_MONO_LABEL,
        }
    }

    const fn hint(self) -> &'static str {
        match self {
            Self::Interface => INTERFACE_HINT,
            Self::Editor | Self::Terminal => CODE_HINT,
        }
    }

    const fn key(self) -> u64 {
        match self {
            Self::Interface => 0,
            Self::Editor => 1,
            Self::Terminal => 2,
        }
    }
}

impl EnvironmentShell {
    fn font_entries(&self, zone: FontZone) -> Vec<MenuEntry> {
        let current = zone.choice(&self.settings).family.as_deref();
        let families = self.settings_ui.as_ref().map_or(&[][..], |ui| match zone {
            FontZone::Interface => ui.fonts.interface.as_slice(),
            FontZone::Editor | FontZone::Terminal => ui.fonts.monospace.as_slice(),
        });
        std::iter::once(MenuEntry::checked(
            zone.default_label(),
            current.is_none(),
            MenuAction::SetFont(zone, None),
        ))
        .chain(families.iter().map(|family| {
            MenuEntry::checked(
                family.clone(),
                current == Some(family.as_str()),
                MenuAction::SetFont(zone, Some(family.clone())),
            )
        }))
        .collect()
    }

    pub(in crate::environment_shell) fn set_font(
        &mut self,
        zone: FontZone,
        edit: impl FnOnce(&mut FontChoice),
        cx: &mut Context<Self>,
    ) {
        edit(zone.choice_mut(&mut self.settings));
        match zone {
            FontZone::Interface => apply_interface_font(&self.settings.interface_font, cx),
            FontZone::Editor => {
                apply_editor_font(&self.settings.editor_font, cx);
            }
            FontZone::Terminal => self.cell_width_px = None,
        }
        self.save_settings(cx);
        cx.notify();
    }

    fn family_picker(&self, zone: FontZone, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let shown = zone
            .choice(&self.settings)
            .family
            .clone()
            .unwrap_or_else(|| zone.default_label().to_owned());
        let entries = self.font_entries(zone);
        div()
            .id((PICKER_PREFIX, zone.key()))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .w(px(PICKER_W))
            .h(px(PICKER_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_SM))
            .border_1()
            .border_color(pal.border)
            .bg(pal.bg_hover)
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .on_click(
                cx.listener(move |this: &mut Self, event: &ClickEvent, window, cx| {
                    let Point { x, y }: Point<Pixels> = event.position();
                    this.open_searchable_menu(
                        (x.as_f32(), y.as_f32()),
                        entries.clone(),
                        MenuSearch {
                            placeholder: FILTER_PLACEHOLDER,
                            create: None,
                            footer: Vec::new(),
                        },
                        window,
                        cx,
                    );
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_strong))
                    .child(shown),
            )
            .child(icon(IconName::ChevronDown, ICON_SM, pal.text_muted))
            .into_any_element()
    }

    fn size_stepper(
        &self,
        zone: FontZone,
        default: FontSize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let size = zone.choice(&self.settings).size_or(default);
        let step = move |grow: bool| {
            move |this: &mut Self,
                  _: &ClickEvent,
                  _: &mut gpui_kit::Window,
                  cx: &mut Context<Self>| {
                this.set_font(
                    zone,
                    |choice| {
                        let points = choice.size_or(default).points();
                        let next = if grow {
                            points.saturating_add(SIZE_STEP)
                        } else {
                            points.saturating_sub(SIZE_STEP)
                        };
                        choice.size = Some(FontSize::new(next));
                    },
                    cx,
                );
            }
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1))
            .child(
                Button::new((SMALLER_PREFIX, zone.key()))
                    .icon(IconName::Minus)
                    .small()
                    .on_click(cx.listener(step(false))),
            )
            .child(
                div()
                    .w(px(SIZE_W))
                    .text_center()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_strong))
                    .child(size.points().to_string()),
            )
            .child(
                Button::new((LARGER_PREFIX, zone.key()))
                    .icon(IconName::Add)
                    .small()
                    .on_click(cx.listener(step(true))),
            )
            .into_any_element()
    }

    fn ligatures_switch(&self, zone: FontZone, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = zone.choice(&self.settings).ligatures;
        let options = Ligatures::all().map(|ligatures| {
            segment(
                (LIGATURES_PREFIX, zone.key() * 2 + ligatures as u64),
                ligatures.display_name(),
                None,
                current == ligatures,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.set_font(zone, |choice| choice.ligatures = ligatures, cx);
            }))
        });
        segmented_track(pal).children(options).into_any_element()
    }

    pub(super) fn font_card(&self, zone: FontZone, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let mut rows = vec![form_row(
            FAMILY_LABEL,
            Some(zone.hint()),
            self.family_picker(zone, cx),
            pal,
        )];
        if let Some(default) = zone.default_size() {
            rows.push(form_row(
                SIZE_LABEL,
                None,
                self.size_stepper(zone, default, cx),
                pal,
            ));
            rows.push(form_row(
                LIGATURES_LABEL,
                Some(LIGATURES_HINT),
                self.ligatures_switch(zone, cx),
                pal,
            ));
        }
        panel_card(rows, pal)
    }
}
