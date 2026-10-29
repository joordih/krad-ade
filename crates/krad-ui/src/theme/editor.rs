use gpui_kit::component::highlighter::{HighlightTheme, HighlightThemeStyle};
use gpui_kit::component::theme::{Theme, ThemeMode, ThemeRegistry};
use gpui_kit::{App, Global};
use krad_domain::{TerminalFont, TerminalTheme};
use krad_session::AnsiPalette;
use std::sync::Arc;

use super::{appearance, palette, terminal_fill_alpha, terminal_font_family, Appearance};
use crate::terminal_theme::terminal_colors;

const ALPHA_BITS: u32 = 8;
const OPAQUE_ALPHA: u32 = 0xff;
const CLEAR_ALPHA: u32 = 0x00;
const ACTIVE_LINE_ALPHA: u32 = 0x0d;
const THEME_SET_NAME: &str = "krad-editor";
const CLEAR_GUTTER: &str = "#00000000";
const THEME_NAME_PREFIX: &str = "krad-editor";
const DARK_MODE: &str = "dark";
const LIGHT_MODE: &str = "light";
const ITALIC: &str = "italic";
const BOLD_WEIGHT: u16 = 700;

const RED: usize = 1;
const GREEN: usize = 2;
const YELLOW: usize = 3;
const BLUE: usize = 4;
const MAGENTA: usize = 5;
const CYAN: usize = 6;
const BRIGHT_BLACK: usize = 8;
const BRIGHT_BLUE: usize = 12;

#[derive(Clone, Copy, Default)]
struct CodeLook {
    theme: TerminalTheme,
    font: TerminalFont,
}

impl Global for CodeLook {}

pub fn apply_code_look(theme: TerminalTheme, font: TerminalFont, cx: &mut App) {
    cx.set_global(CodeLook { theme, font });
    style_editor(cx);
    cx.refresh_windows();
}

fn hex(color: u32, alpha: u32) -> String {
    format!("#{:08x}", (color << ALPHA_BITS) | alpha)
}

fn syntax_entry(name: &str, color: u32, face: Face) -> String {
    let color = hex(color, OPAQUE_ALPHA);
    match face {
        Face::Plain => format!(r#""{name}":{{"color":"{color}"}}"#),
        Face::Italic => format!(r#""{name}":{{"color":"{color}","font_style":"{ITALIC}"}}"#),
        Face::Bold => {
            format!(r#""{name}":{{"color":"{color}","font_weight":{BOLD_WEIGHT}}}"#)
        }
    }
}

#[derive(Clone, Copy)]
enum Face {
    Plain,
    Italic,
    Bold,
}

fn highlight_json(name: &str, mode: &str, background: &str, colors: &Colors) -> String {
    let ansi = |slot: usize| {
        colors
            .ansi
            .0
            .get(slot)
            .copied()
            .unwrap_or(colors.foreground)
    };
    let syntax = [
        ("comment", ansi(BRIGHT_BLACK), Face::Italic),
        ("comment.doc", ansi(BRIGHT_BLACK), Face::Italic),
        ("keyword", ansi(MAGENTA), Face::Plain),
        ("preproc", ansi(MAGENTA), Face::Plain),
        ("label", ansi(MAGENTA), Face::Plain),
        ("string", ansi(GREEN), Face::Plain),
        ("string.escape", ansi(CYAN), Face::Plain),
        ("string.regex", ansi(CYAN), Face::Plain),
        ("string.special", ansi(CYAN), Face::Plain),
        ("number", ansi(YELLOW), Face::Plain),
        ("boolean", ansi(YELLOW), Face::Plain),
        ("constant", ansi(YELLOW), Face::Plain),
        ("attribute", ansi(YELLOW), Face::Plain),
        ("variant", ansi(YELLOW), Face::Plain),
        ("type", ansi(CYAN), Face::Plain),
        ("enum", ansi(CYAN), Face::Plain),
        ("operator", ansi(CYAN), Face::Plain),
        ("function", ansi(BLUE), Face::Plain),
        ("constructor", ansi(BLUE), Face::Plain),
        ("link_uri", ansi(BLUE), Face::Plain),
        ("title", ansi(BLUE), Face::Bold),
        ("property", ansi(BRIGHT_BLUE), Face::Plain),
        ("tag", ansi(RED), Face::Plain),
        ("variable.special", ansi(RED), Face::Plain),
        ("variable", colors.foreground, Face::Plain),
        ("punctuation", colors.foreground, Face::Plain),
        ("embedded", colors.foreground, Face::Plain),
    ]
    .map(|(entry, color, face)| syntax_entry(entry, color, face))
    .join(",");
    let foreground = hex(colors.foreground, OPAQUE_ALPHA);
    let active = hex(colors.foreground, ACTIVE_LINE_ALPHA);
    let line_number = hex(ansi(BRIGHT_BLACK), OPAQUE_ALPHA);
    format!(
        r#"{{"name":"{THEME_SET_NAME}","themes":[{{"name":"{name}","mode":"{mode}","highlight":{{"editor.background":"{background}","editor.gutter.background":"{CLEAR_GUTTER}","editor.foreground":"{foreground}","editor.active_line.background":"{active}","editor.line_number":"{line_number}","editor.active_line_number":"{foreground}","syntax":{{{syntax}}}}}}}]}}"#
    )
}

fn parse_highlight(name: &str, json: &str) -> Option<HighlightThemeStyle> {
    let mut parser = ThemeRegistry::default();
    parser.load_themes_from_str(json).ok()?;
    parser
        .themes()
        .get(name)
        .and_then(|config| config.highlight.clone())
}

struct Colors {
    background: u32,
    foreground: u32,
    ansi: AnsiPalette,
}

pub(super) fn style_editor(cx: &mut App) {
    let look = cx.try_global::<CodeLook>().copied().unwrap_or_default();
    let pal = palette(cx);
    let terminal = terminal_colors(look.theme, pal);
    let colors = Colors {
        background: terminal.background,
        foreground: terminal.foreground,
        ansi: terminal.ansi,
    };
    let dark = appearance(cx) == Appearance::Dark;
    let alpha = terminal_fill_alpha(colors.background, pal, cx).unwrap_or(CLEAR_ALPHA);
    let background = hex(colors.background, alpha);
    let (mode, kit_mode) = if dark {
        (DARK_MODE, ThemeMode::Dark)
    } else {
        (LIGHT_MODE, ThemeMode::Light)
    };
    let name = format!("{THEME_NAME_PREFIX}-{:?}-{mode}", look.theme);
    let style = parse_highlight(&name, &highlight_json(&name, mode, &background, &colors));
    let theme = Theme::global_mut(cx);
    theme.mono_font_family = terminal_font_family(look.font).into();
    if let Some(style) = style {
        theme.highlight_theme = Arc::new(HighlightTheme {
            name,
            appearance: kit_mode,
            style,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEMED_TERMINAL_ALPHA: u32 = 0x1a;

    #[test]
    fn gpui_component_reads_the_highlight_theme_with_its_colors() {
        let colors = Colors {
            background: 0x0019_171b,
            foreground: 0x00ed_eaf0,
            ansi: AnsiPalette([0x0011_2233; 16]),
        };
        let background = hex(0x0028_2828, THEMED_TERMINAL_ALPHA);
        let json = highlight_json("t", DARK_MODE, &background, &colors);
        let style = parse_highlight("t", &json);
        let alphas = style.map(|style| {
            (
                style.editor_background.map(|color| color.a > 0.0),
                style.editor_gutter_background.map(|color| color.a),
            )
        });
        assert_eq!(
            alphas,
            Some((Some(true), Some(0.0))),
            "the gutter adds nothing over the editor's own translucent background"
        );
    }

    #[test]
    fn the_highlight_theme_is_one_balanced_json_document() {
        let colors = Colors {
            background: 0x0019_171b,
            foreground: 0x00ed_eaf0,
            ansi: AnsiPalette([0x0011_2233; 16]),
        };
        let json = highlight_json("t", DARK_MODE, &hex(0x0019_171b, CLEAR_ALPHA), &colors);
        let opened = json.matches('{').count();
        assert_eq!(opened, json.matches('}').count(), "{json}");
        assert!(
            json.contains(r##""editor.background":"#19171b00""##),
            "{json}"
        );
        assert!(
            json.contains(r##""comment":{"color":"#112233ff","font_style":"italic"}"##),
            "{json}"
        );
        assert!(json.contains(r#""font_weight":700"#), "{json}");
        assert!(!json.contains(",,"), "{json}");
    }
}
