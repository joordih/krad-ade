use gpui_kit::{div, px, rgb, AnyElement, IntoElement, ParentElement, SharedString, Styled};
use krad_domain::Theme;

use crate::theme::{
    LINE_SM, LINE_XS, RADIUS_MD, RADIUS_SM, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};

const PREVIEW_TITLE: &str = "Session";
const PREVIEW_DETAIL: &str = "3 agents working";
const PREVIEW_BUTTON: &str = "Send";
const PROMPT: &str = "~ $ cargo run";
const DOT: f32 = 8.0;

fn token(text: &'static str, color: u32) -> AnyElement {
    div().text_color(rgb(color)).child(text).into_any_element()
}

fn code_lines(theme: &Theme) -> Vec<AnyElement> {
    let code = &theme.syntax;
    let line = |parts: Vec<AnyElement>| div().flex().flex_row().children(parts).into_any_element();
    vec![
        line(vec![token("// greet the user", code.comment.0)]),
        line(vec![
            token("fn ", code.keyword.0),
            token("greet", code.function.0),
            token("(", code.punctuation.0),
            token("name", code.variable.0),
            token(": ", code.punctuation.0),
            token("&str", code.type_name.0),
            token(") {", code.punctuation.0),
        ]),
        line(vec![
            token("    let ", code.keyword.0),
            token("count", code.variable.0),
            token(" = ", code.operator.0),
            token("42", code.number.0),
            token(";", code.punctuation.0),
        ]),
        line(vec![
            token("    println!", code.function.0),
            token("(", code.punctuation.0),
            token("\"hi {name}\\n\"", code.string.0),
            token(");", code.punctuation.0),
        ]),
        line(vec![token("}", code.punctuation.0)]),
    ]
}

pub(super) fn theme_preview(theme: &Theme, mono: SharedString) -> AnyElement {
    let ui = &theme.interface;
    let term = &theme.terminal;
    let statuses = [ui.success, ui.warning, ui.error, ui.info];
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_2))
        .p(px(SPACE_3))
        .rounded(px(RADIUS_MD))
        .bg(rgb(ui.background.0))
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_2))
                .p(px(SPACE_2))
                .rounded(px(RADIUS_SM))
                .bg(rgb(ui.surface.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_color(rgb(ui.text.0)).child(PREVIEW_TITLE))
                        .child(
                            div()
                                .text_size(px(TEXT_XS))
                                .line_height(px(LINE_XS))
                                .text_color(rgb(ui.muted_text.0))
                                .child(PREVIEW_DETAIL),
                        ),
                )
                .children(statuses.map(|color| div().size(px(DOT)).rounded_full().bg(rgb(color.0))))
                .child(
                    div()
                        .px(px(SPACE_2))
                        .py(px(SPACE_1))
                        .rounded(px(RADIUS_SM))
                        .bg(rgb(ui.accent.0))
                        .text_color(rgb(ui.on_accent.0))
                        .child(PREVIEW_BUTTON),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .p(px(SPACE_2))
                .rounded(px(RADIUS_SM))
                .bg(rgb(ui.panel.0))
                .font_family(mono.clone())
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .children(code_lines(theme)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_1_5))
                .p(px(SPACE_2))
                .rounded(px(RADIUS_SM))
                .bg(rgb(term.background.0))
                .font_family(mono)
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(SPACE_1))
                        .child(div().text_color(rgb(term.foreground.0)).child(PROMPT))
                        .child(div().w(px(DOT)).bg(rgb(term.cursor.0))),
                )
                .child(
                    div().flex().flex_row().gap(px(SPACE_1)).children(
                        term.ansi
                            .iter()
                            .map(|color| div().size(px(DOT)).rounded_full().bg(rgb(color.0))),
                    ),
                ),
        )
        .into_any_element()
}
