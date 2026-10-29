use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{CopyMode, Ligatures, TerminalFont, TerminalTheme};

use super::super::dialog_parts::{field_label, segment, SegmentSize};
use super::{
    form_row, hint, panel_card, segmented_track, swatch_grid, EnvironmentShell, SWATCH_GAP,
    SWATCH_PAD, SWATCH_PREVIEW_H,
};
use crate::keymap::{COPY_HINT, PASTE_HINT};
use crate::terminal_theme::terminal_colors;
use crate::theme::{
    apply_code_look, focus_ring, palette, row_shadow, LINE_XS, RADIUS_MD, SPACE_1, SPACE_1_5,
    SYSTEM_MONO_LABEL, TEXT_XS,
};

const SWATCH_PREFIX: &str = "theme-swatch";
const SWATCH_DOT: f32 = 8.0;
const SWATCH_SAMPLE: &str = "~ $ cargo run";
const SWATCH_ANSI: std::ops::RangeInclusive<usize> = 1..=6;
const THEME_SECTION: &str = "Terminal and editor theme";
const THEME_HINT: &str = "Colors of the terminals and the code editor, apart from the app theme.";
const COPY_MODE_PREFIX: &str = "copy-mode";
const COPY_LABEL: &str = "Copy";
const FONT_PREFIX: &str = "terminal-font";
const FONT_LABEL: &str = "Font";
const FONT_HINT: &str = "JetBrains Mono draws ligatures; the system font does not.";
const JETBRAINS_MONO_LABEL: &str = "JetBrains Mono";

const fn font_label(font: TerminalFont) -> &'static str {
    match font {
        TerminalFont::System => SYSTEM_MONO_LABEL,
        TerminalFont::JetBrainsMono => JETBRAINS_MONO_LABEL,
    }
}
const LIGATURES_PREFIX: &str = "ligatures";
const LIGATURES_LABEL: &str = "Ligatures";
const LIGATURES_HINT: &str =
    "Join sequences such as => or != into one glyph, in fonts that draw them.";

impl EnvironmentShell {
    pub(super) fn terminal_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let grid = swatch_grid(
            TerminalTheme::all()
                .into_iter()
                .map(|theme| self.theme_swatch(theme, cx))
                .collect(),
        );
        div()
            .flex()
            .flex_col()
            .gap(px(SWATCH_GAP))
            .child(self.copy_mode_card(cx))
            .child(self.font_card(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1))
                    .child(field_label(THEME_SECTION, pal))
                    .child(hint(THEME_HINT, pal)),
            )
            .children(grid)
            .into_any_element()
    }

    fn copy_mode_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.copy_mode;
        let options = CopyMode::all().map(|mode| {
            segment(
                (COPY_MODE_PREFIX, mode as u64),
                mode.display_name(),
                None,
                current == mode,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.settings.copy_mode = mode;
                this.save_settings(cx);
                cx.notify();
            }))
        });
        let hint =
            format!("Copy with {COPY_HINT}, by selecting text, or both. Paste with {PASTE_HINT}.");
        panel_card(
            [(form_row(
                COPY_LABEL,
                Some(&hint),
                segmented_track(pal).children(options).into_any_element(),
                pal,
            ))
            .into_any_element()],
            pal,
        )
        .into_any_element()
    }

    fn font_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let font = self.settings.terminal_font;
        let fonts = TerminalFont::all().map(|choice| {
            segment(
                (FONT_PREFIX, choice as u64),
                font_label(choice),
                None,
                font == choice,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.settings.terminal_font = choice;
                apply_code_look(this.settings.terminal_theme, choice, cx);
                this.cell_width_px = None;
                this.save_settings(cx);
                cx.notify();
            }))
        });
        let current = self.settings.ligatures;
        let options = Ligatures::all().map(|ligatures| {
            segment(
                (LIGATURES_PREFIX, ligatures as u64),
                ligatures.display_name(),
                None,
                current == ligatures,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.settings.ligatures = ligatures;
                this.save_settings(cx);
                cx.notify();
            }))
        });
        panel_card(
            [
                (form_row(
                    FONT_LABEL,
                    Some(FONT_HINT),
                    segmented_track(pal).children(fonts).into_any_element(),
                    pal,
                ))
                .into_any_element(),
                (form_row(
                    LIGATURES_LABEL,
                    Some(LIGATURES_HINT),
                    segmented_track(pal).children(options).into_any_element(),
                    pal,
                ))
                .into_any_element(),
            ],
            pal,
        )
        .into_any_element()
    }

    fn theme_swatch(&self, theme: TerminalTheme, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let colors = terminal_colors(theme, pal);
        let selected = self.settings.terminal_theme == theme;
        div()
            .id((SWATCH_PREFIX, theme as u64))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(SPACE_1_5))
            .cursor_pointer()
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.settings.terminal_theme = theme;
                apply_code_look(theme, this.settings.terminal_font, cx);
                this.save_settings(cx);
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .justify_between()
                    .h(px(SWATCH_PREVIEW_H))
                    .p(px(SWATCH_PAD))
                    .rounded(px(RADIUS_MD))
                    .bg(rgb(colors.background))
                    .shadow(if selected {
                        focus_ring(pal)
                    } else {
                        row_shadow(pal, cx)
                    })
                    .font_family(mono)
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(colors.foreground))
                    .child(SWATCH_SAMPLE)
                    .child(
                        div().flex().flex_row().gap(px(SPACE_1)).children(
                            colors.ansi.0[SWATCH_ANSI].iter().map(|color| {
                                div().size(px(SWATCH_DOT)).rounded_full().bg(rgb(*color))
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .font_weight(if selected {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(rgb(pal.text_strong))
                    .child(theme.display_name()),
            )
            .into_any_element()
    }
}
