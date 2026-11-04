use gpui_kit::component::highlighter::{HighlightTheme, HighlightThemeStyle};
use gpui_kit::component::theme::{Theme, ThemeMode, ThemeRegistry};
use gpui_kit::{App, Global, SharedString};
use krad_domain::{FontChoice, SyntaxColors, ThemeColor};
use std::sync::Arc;

use super::{appearance, current_theme, mono_family, palette, terminal_fill_alpha, Appearance};

const ALPHA_BITS: u32 = 8;
const OPAQUE_ALPHA: u32 = 0xff;
const CLEAR_ALPHA: u32 = 0x00;
const ACTIVE_LINE_ALPHA: u32 = 0x0d;
const THEME_SET_NAME: &str = "krad-editor";
const THEME_NAME_PREFIX: &str = "krad-editor";
const DARK_MODE: &str = "dark";
const LIGHT_MODE: &str = "light";
const ITALIC: &str = "italic";
const BOLD_WEIGHT: u16 = 700;

#[derive(Clone, Default)]
struct CodeLook {
    family: SharedString,
}

impl Global for CodeLook {}

pub fn apply_editor_font(editor: &FontChoice, cx: &mut App) {
    cx.set_global(CodeLook {
        family: mono_family(editor),
    });
    style_editor(cx);
    cx.refresh_windows();
}

fn alpha_fraction(alpha: u32) -> f32 {
    let alpha = u16::try_from(alpha.min(OPAQUE_ALPHA)).unwrap_or_default();
    f32::from(alpha) / f32::from(u16::try_from(OPAQUE_ALPHA).unwrap_or(1))
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

fn highlight_json(
    name: &str,
    mode: &str,
    (background, gutter): (&str, &str),
    colors: &Colors,
) -> String {
    let code = &colors.syntax;
    let syntax = [
        ("comment", code.comment.0, Face::Italic),
        ("comment.doc", code.comment.0, Face::Italic),
        ("keyword", code.keyword.0, Face::Plain),
        ("preproc", code.keyword.0, Face::Plain),
        ("label", code.keyword.0, Face::Plain),
        ("string", code.string.0, Face::Plain),
        ("string.escape", code.escape.0, Face::Plain),
        ("string.regex", code.escape.0, Face::Plain),
        ("string.special", code.escape.0, Face::Plain),
        ("number", code.number.0, Face::Plain),
        ("boolean", code.constant.0, Face::Plain),
        ("constant", code.constant.0, Face::Plain),
        ("attribute", code.attribute.0, Face::Plain),
        ("variant", code.constant.0, Face::Plain),
        ("type", code.type_name.0, Face::Plain),
        ("enum", code.type_name.0, Face::Plain),
        ("operator", code.operator.0, Face::Plain),
        ("function", code.function.0, Face::Plain),
        ("constructor", code.function.0, Face::Plain),
        ("link_uri", code.function.0, Face::Plain),
        ("title", code.function.0, Face::Bold),
        ("property", code.property.0, Face::Plain),
        ("tag", code.tag.0, Face::Plain),
        ("variable.special", code.tag.0, Face::Plain),
        ("variable", code.variable.0, Face::Plain),
        ("punctuation", code.punctuation.0, Face::Plain),
        ("embedded", colors.foreground, Face::Plain),
    ]
    .map(|(entry, color, face)| syntax_entry(entry, color, face))
    .join(",");
    let foreground = hex(colors.foreground, OPAQUE_ALPHA);
    let active = hex(colors.foreground, ACTIVE_LINE_ALPHA);
    let line_number = hex(code.comment.0, OPAQUE_ALPHA);
    format!(
        r#"{{"name":"{THEME_SET_NAME}","themes":[{{"name":"{name}","mode":"{mode}","highlight":{{"editor.background":"{background}","editor.gutter.background":"{gutter}","editor.foreground":"{foreground}","editor.active_line.background":"{active}","editor.line_number":"{line_number}","editor.active_line_number":"{foreground}","syntax":{{{syntax}}}}}}}]}}"#
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
    syntax: SyntaxColors,
}

pub(super) fn style_editor(cx: &mut App) {
    let look = cx.try_global::<CodeLook>().cloned().unwrap_or_default();
    let pal = palette(cx);
    let current = current_theme(cx);
    let colors = Colors {
        background: current.terminal.background.0,
        foreground: current.terminal.foreground.0,
        syntax: current.syntax,
    };
    let theme_id = current.id.0.clone();
    let dark = appearance(cx) == Appearance::Dark;
    let alpha = terminal_fill_alpha(colors.background, pal, cx).unwrap_or(CLEAR_ALPHA);
    let background = hex(colors.background, alpha);
    let covered = alpha_fraction(alpha);
    let gutter = hex(
        ThemeColor(pal.bg_surface)
            .mix(ThemeColor(colors.background), covered)
            .0,
        OPAQUE_ALPHA,
    );
    let (mode, kit_mode) = if dark {
        (DARK_MODE, ThemeMode::Dark)
    } else {
        (LIGHT_MODE, ThemeMode::Light)
    };
    let name = format!("{THEME_NAME_PREFIX}-{theme_id}-{mode}");
    let style = parse_highlight(
        &name,
        &highlight_json(&name, mode, (&background, &gutter), &colors),
    );
    let theme = Theme::global_mut(cx);
    theme.mono_font_family = look.family;
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
    use gpui_kit::{rgba, FontStyle, FontWeight, Hsla};

    const THEMED_TERMINAL_ALPHA: u32 = 0x1a;
    const COMMENT_GREY: u32 = 0x0044_5566;
    const COMMENT_GREY_RGBA: u32 = 0x4455_66ff;

    fn current_theme_for_tests() -> krad_domain::Theme {
        include_str!("../../../../assets/themes/library/dracula.json")
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn gpui_component_reads_the_highlight_theme_with_its_colors() {
        let mut syntax = current_theme_for_tests().syntax;
        syntax.comment = krad_domain::ThemeColor(COMMENT_GREY);
        let colors = Colors {
            background: 0x0019_171b,
            foreground: 0x00ed_eaf0,
            syntax,
        };
        let background = hex(0x0028_2828, THEMED_TERMINAL_ALPHA);
        let gutter = hex(0x0028_2828, OPAQUE_ALPHA);
        let json = highlight_json("t", DARK_MODE, (&background, &gutter), &colors);
        let style = parse_highlight("t", &json);
        let alphas = style.as_ref().map(|style| {
            (
                style.editor_background.map(|color| color.a > 0.0),
                style.editor_gutter_background.map(|color| color.a),
            )
        });
        assert_eq!(
            alphas,
            Some((Some(true), Some(1.0))),
            "the gutter is opaque so scrolled text never shows under the line numbers"
        );
        let syntax = |name: &str| style.as_ref().and_then(|style| style.syntax.style(name));
        let comment = syntax("comment");
        assert_eq!(
            comment.and_then(|comment| comment.color),
            Some(Hsla::from(rgba(COMMENT_GREY_RGBA))),
            "comments take the theme's comment color"
        );
        assert_eq!(
            comment.and_then(|comment| comment.font_style),
            Some(FontStyle::Italic)
        );
        assert_eq!(
            syntax("title").and_then(|title| title.font_weight),
            Some(FontWeight::BOLD)
        );
    }
}
