use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{Theme, ThemeAppearance};

use super::super::dialog_parts::field_label;
use super::{hint, swatch_grid, EnvironmentShell, SWATCH_GAP, SWATCH_PAD, SWATCH_PREVIEW_H};
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{
    apply_settings_theme, focus_ring, palette, row_shadow, theme_library, LINE_XS, RADIUS_MD,
    SPACE_1, SPACE_1_5, TEXT_XS,
};

const SWATCH_PREFIX: &str = "theme-swatch";
const SWATCH_DOT: f32 = 8.0;
const SWATCH_SAMPLE: &str = "fn main() {}";
const SWATCH_ANSI: std::ops::RangeInclusive<usize> = 1..=6;
const LIGHT_SECTION: &str = "Light theme";
const DARK_SECTION: &str = "Dark theme";
const LIGHT_HINT: &str =
    "Used while the app looks light. It colors the interface, terminals and editor.";
const DARK_HINT: &str =
    "Used while the app looks dark. It colors the interface, terminals and editor.";
const CREDIT_SEPARATOR: &str = " · ";
const OWN_THEME: &str = "Krad";
const BTN_CREATE_THEME: &str = "theme-create";
const CREATE_THEME_LABEL: &str = "Create theme…";

fn credit_line(theme: &Theme) -> String {
    theme.credit.as_ref().map_or_else(
        || OWN_THEME.to_owned(),
        |credit| format!("{}{CREDIT_SEPARATOR}{}", credit.author, credit.license),
    )
}

impl EnvironmentShell {
    pub(super) fn select_theme(&mut self, theme: &Theme, cx: &mut Context<Self>) {
        self.settings
            .themes
            .get_or_insert_with(Default::default)
            .set(theme.appearance, theme.id.clone());
        apply_settings_theme(&self.settings, cx);
        self.save_settings(cx);
        cx.notify();
    }

    fn theme_swatch(&self, index: usize, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let selected = self
            .settings
            .themes
            .clone()
            .unwrap_or_default()
            .for_appearance(theme.appearance)
            == &theme.id;
        let chosen = theme.clone();
        let terminal = theme.terminal;
        div()
            .id((SWATCH_PREFIX, u64::try_from(index).unwrap_or(u64::MAX)))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(SPACE_1_5))
            .cursor_pointer()
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.select_theme(&chosen, cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .justify_between()
                    .h(px(SWATCH_PREVIEW_H))
                    .p(px(SWATCH_PAD))
                    .rounded(px(RADIUS_MD))
                    .bg(rgb(theme.interface.surface.0))
                    .shadow(if selected {
                        focus_ring(pal)
                    } else {
                        row_shadow(pal, cx)
                    })
                    .font_family(mono)
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(theme.syntax.keyword.0))
                    .child(SWATCH_SAMPLE)
                    .child(div().flex().flex_row().gap(px(SPACE_1)).children(
                        terminal.ansi[SWATCH_ANSI].iter().map(|color| {
                            div().size(px(SWATCH_DOT)).rounded_full().bg(rgb(color.0))
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .child(
                        div()
                            .truncate()
                            .font_weight(if selected {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(rgb(pal.text_strong))
                            .child(theme.name.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_color(rgb(pal.text_muted))
                            .child(credit_line(theme)),
                    ),
            )
            .into_any_element()
    }

    fn theme_group(&self, appearance: ThemeAppearance, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let (title, detail) = match appearance {
            ThemeAppearance::Light => (LIGHT_SECTION, LIGHT_HINT),
            ThemeAppearance::Dark => (DARK_SECTION, DARK_HINT),
        };
        let themes: Vec<(usize, Theme)> = theme_library(cx)
            .iter()
            .enumerate()
            .filter(|(_, theme)| theme.appearance == appearance)
            .map(|(index, theme)| (index, theme.clone()))
            .collect();
        let swatches = themes
            .iter()
            .map(|(index, theme)| self.theme_swatch(*index, theme, cx))
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(SWATCH_GAP))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1))
                    .child(field_label(title, pal))
                    .child(hint(detail, pal)),
            )
            .children(swatch_grid(swatches))
            .into_any_element()
    }

    pub(super) fn themes_section(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(SWATCH_GAP))
            .child(
                div().flex().flex_row().justify_end().child(
                    Button::new(BTN_CREATE_THEME)
                        .label(CREATE_THEME_LABEL)
                        .icon(IconName::Add)
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                            this.open_theme_editor(window, cx);
                        })),
                ),
            )
            .child(self.theme_group(ThemeAppearance::Light, cx))
            .child(self.theme_group(ThemeAppearance::Dark, cx))
            .into_any_element()
    }
}
