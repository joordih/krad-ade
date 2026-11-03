use gpui_kit::component::input::{EditorState, RopeExt as _};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, Entity, FontWeight, HighlightStyle, IntoElement,
    ParentElement, SharedString, Styled, StyledText,
};
use krad_lsp::lsp_types::{
    Documentation, MarkupContent, ParameterInformation, ParameterLabel, SignatureHelp,
};
use std::ops::Range;

use super::super::language_servers::Document;
use super::super::EnvironmentShell;
use super::{place_above, placed, popover_panel};
use crate::theme::{palette, LINE_SM, LINE_XS, SPACE_1, SPACE_2, TEXT_SM, TEXT_XS};

const CARD_W: f32 = 520.0;
const CARD_MAX_H: f32 = 200.0;
const CLOSER: char = ')';
const COUNT_SEPARATOR: &str = "/";

pub(in crate::environment_shell) struct SignatureCard {
    tab: krad_domain::TerminalId,
    at: usize,
    label: String,
    active: Option<Range<usize>>,
    docs: Option<String>,
    position: Option<(usize, usize)>,
}

fn utf16_to_byte(text: &str, utf16: u32) -> usize {
    let mut seen = 0u32;
    for (index, ch) in text.char_indices() {
        if seen >= utf16 {
            return index;
        }
        seen += u32::try_from(ch.len_utf16()).unwrap_or(1);
    }
    text.len()
}

pub(super) fn parameter_range(
    label: &str,
    parameter: &ParameterInformation,
) -> Option<Range<usize>> {
    match &parameter.label {
        ParameterLabel::Simple(name) => {
            let after = label.find('(').map_or(0, |open| open + 1);
            label[after..]
                .find(name.as_str())
                .map(|start| (after + start)..(after + start + name.len()))
        }
        ParameterLabel::LabelOffsets([start, end]) => {
            Some(utf16_to_byte(label, *start)..utf16_to_byte(label, *end))
        }
    }
}

fn docs_of(documentation: Option<&Documentation>) -> Option<String> {
    match documentation? {
        Documentation::String(text) => Some(text.clone()),
        Documentation::MarkupContent(MarkupContent { value, .. }) => Some(value.clone()),
    }
    .filter(|text| !text.trim().is_empty())
}

pub(super) fn card_parts(
    help: &SignatureHelp,
) -> Option<(String, Option<Range<usize>>, Option<String>)> {
    let index = help.active_signature.unwrap_or(0) as usize;
    let signature = help
        .signatures
        .get(index)
        .or_else(|| help.signatures.first())?;
    let active = signature
        .active_parameter
        .or(help.active_parameter)
        .and_then(|parameter| signature.parameters.as_ref()?.get(parameter as usize));
    let range = active.and_then(|parameter| parameter_range(&signature.label, parameter));
    let docs = active
        .and_then(|parameter| docs_of(parameter.documentation.as_ref()))
        .or_else(|| docs_of(signature.documentation.as_ref()));
    Some((signature.label.clone(), range, docs))
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn signature_typed(
        &mut self,
        document: &Document,
        state: &Entity<EditorState>,
        cx: &mut Context<Self>,
    ) {
        let editor = state.read(cx);
        let cursor = editor.cursor();
        let before = editor
            .text()
            .slice(cursor.saturating_sub(4)..cursor)
            .to_string()
            .chars()
            .last();
        if before == Some(CLOSER) {
            self.close_signature(cx);
            return;
        }
        let triggered =
            before.is_some_and(|typed| self.signature_triggers(document).contains(&typed));
        if !triggered && self.signature.is_none() {
            return;
        }
        let position = editor.text().offset_to_position(cursor);
        let Some(asked) = self.request_signature_help(document, position, cx) else {
            return;
        };
        let document = document.clone();
        cx.spawn(async move |this, cx| {
            let help = asked.await;
            this.update(cx, |shell, cx| {
                shell.show_signature(&document, cursor, help, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn show_signature(
        &mut self,
        document: &Document,
        at: usize,
        help: Option<SignatureHelp>,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_showing(document) else {
            return;
        };
        let Some(help) = help.filter(|help| !help.signatures.is_empty()) else {
            self.close_signature(cx);
            return;
        };
        let Some((label, active, docs)) = card_parts(&help) else {
            self.close_signature(cx);
            return;
        };
        let count = help.signatures.len();
        let position =
            (count > 1).then(|| (help.active_signature.unwrap_or(0) as usize + 1, count));
        self.signature = Some(SignatureCard {
            tab,
            at,
            label,
            active,
            docs,
            position,
        });
        cx.notify();
    }

    pub(in crate::environment_shell) fn close_signature(&mut self, cx: &mut Context<Self>) {
        if self.signature.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::environment_shell) fn signature_overlay(
        &self,
        window: &gpui_kit::Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let card = self.signature.as_ref()?;
        let (state, _) = self.popover_editor(card.tab)?;
        let anchor = Self::anchor_bounds(&state, &(card.at..card.at), cx)?;
        let pal = palette(cx);
        let placement = place_above(
            anchor,
            self.frame_inset,
            self.root_size(window),
            (CARD_W, CARD_MAX_H),
        );
        let highlights: Vec<(Range<usize>, HighlightStyle)> = card
            .active
            .clone()
            .map(|range| {
                (
                    range,
                    HighlightStyle {
                        color: Some(rgb(pal.accent).into()),
                        font_weight: Some(FontWeight::BOLD),
                        ..HighlightStyle::default()
                    },
                )
            })
            .into_iter()
            .collect();
        Some(
            placed(popover_panel(pal, cx), placement)
                .w(px(CARD_W))
                .gap(px(SPACE_1))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(SPACE_2))
                        .px(px(SPACE_2))
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_size(px(TEXT_SM))
                        .line_height(px(LINE_SM))
                        .text_color(rgb(pal.text_strong))
                        .child(
                            div().flex_1().min_w_0().child(
                                StyledText::new(SharedString::from(card.label.clone()))
                                    .with_highlights(highlights),
                            ),
                        )
                        .children(card.position.map(|(index, count)| {
                            div()
                                .flex_none()
                                .text_size(px(TEXT_XS))
                                .text_color(rgb(pal.text_muted))
                                .child(format!("{index}{COUNT_SEPARATOR}{count}"))
                        })),
                )
                .children(card.docs.clone().map(|docs| {
                    div()
                        .px(px(SPACE_2))
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(docs)
                }))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_lsp::lsp_types::SignatureInformation;

    fn help(active: u32) -> SignatureHelp {
        SignatureHelp {
            signatures: vec![SignatureInformation {
                label: "load(File jar, boolean verify)".to_owned(),
                documentation: None,
                parameters: Some(vec![
                    ParameterInformation {
                        label: ParameterLabel::Simple("File jar".to_owned()),
                        documentation: Some(Documentation::String("The plugin jar".to_owned())),
                    },
                    ParameterInformation {
                        label: ParameterLabel::LabelOffsets([15, 29]),
                        documentation: None,
                    },
                ]),
                active_parameter: None,
            }],
            active_signature: Some(0),
            active_parameter: Some(active),
        }
    }

    #[test]
    fn the_active_parameter_is_found_by_name_or_by_offsets() {
        let (label, range, docs) = card_parts(&help(0)).unwrap_or_default();
        assert_eq!(range.map(|range| &label[range]), Some("File jar"));
        assert_eq!(docs.as_deref(), Some("The plugin jar"));
        let (label, range, docs) = card_parts(&help(1)).unwrap_or_default();
        assert_eq!(range.map(|range| &label[range]), Some("boolean verify"));
        assert_eq!(docs, None);
    }
}
