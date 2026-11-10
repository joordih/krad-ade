use gpui_kit::component::input::{EditorState, Position, Rope, RopeExt as _, ToggleCodeActions};
use gpui_kit::component::text::TextView;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    canvas, div, px, rgb, AnyElement, App, Bounds, Context, DispatchPhase, ElementId, Entity,
    FontWeight, HighlightStyle, InteractiveElement, IntoElement, MouseMoveEvent, ParentElement,
    Pixels, Point, SharedString, StatefulInteractiveElement, Styled, StyledText, WeakEntity,
    Window,
};
use krad_domain::{LanguageId, ProblemSeverity};
use krad_lsp::lsp_types::Hover;
use std::ops::Range;
use std::time::Instant;

use super::super::file_tab::problem_list::EditorProblem;
use super::super::language_servers::Document;
use super::super::EnvironmentShell;
use super::content::{hover_blocks, HoverBlock};
use super::{place, placed, popover_panel, EditorPopover};
use crate::components::{icon, Button, IconName};
use crate::highlight::{highlight_lines, Syntax};
use crate::theme::{
    palette, Palette, ICON_SM, LINE_SM, LINE_XS, SPACE_1, SPACE_1_5, SPACE_2, TEXT_SM, TEXT_XS,
};
use pointer::{HoverPointer, PointerStep, PointerZone, HOVER_GRACE};

const HOVER_W: f32 = 520.0;
const HOVER_MAX_H: f32 = 360.0;
const HOVER_ID: &str = "editor-hover";
const MARKDOWN_PREFIX: &str = "editor-hover-text";
const FIXES_PREFIX: &str = "editor-hover-fixes";
const FIXES_LABEL: &str = "Show fixes";
const KEEP_MARGIN_PX: f32 = 6.0;

mod pointer;

pub(in crate::environment_shell) struct HoverCard {
    pub(super) tab: krad_domain::TerminalId,
    anchor: Range<usize>,
    scrolled_at: Point<Pixels>,
    blocks: Vec<HoverBlock>,
    problems: Vec<EditorProblem>,
    syntax: Syntax,
    pointer: HoverPointer,
}

fn word_around(text: &Rope, offset: usize) -> Range<usize> {
    let line = text.offset_to_point(offset).row;
    let start = text.line_start_offset(line);
    let end = text.line_end_offset(line);
    let line_text = text.slice(start..end).to_string();
    let at = offset.saturating_sub(start).min(line_text.len());
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let from = line_text[..at]
        .char_indices()
        .rev()
        .take_while(|(_, ch)| is_word(*ch))
        .last()
        .map_or(at, |(index, _)| index);
    let to = line_text[at..]
        .char_indices()
        .find(|(_, ch)| !is_word(*ch))
        .map_or(line_text.len(), |(index, _)| at + index);
    (start + from)..(start + to.max(from))
}

const fn severity_look(severity: ProblemSeverity, pal: &Palette) -> (IconName, u32) {
    match severity {
        ProblemSeverity::Error => (IconName::CancelCircle, pal.error_fg),
        ProblemSeverity::Warning => (IconName::Flag, pal.warning_fg),
        ProblemSeverity::Info | ProblemSeverity::Hint => (IconName::Comment, pal.info_fg),
    }
}

fn code_block(text: &str, syntax: Syntax, pal: &Palette, cx: &App) -> AnyElement {
    let lines = highlight_lines(text, syntax);
    let rows = text.lines().zip(lines).map(|(line, spans)| {
        let highlights: Vec<(Range<usize>, HighlightStyle)> = spans
            .iter()
            .filter(|span| span.range.end <= line.len())
            .map(|span| {
                (
                    span.range.clone(),
                    HighlightStyle {
                        color: Some(rgb(pal.syntax.of(span.kind)).into()),
                        ..HighlightStyle::default()
                    },
                )
            })
            .collect();
        div()
            .child(StyledText::new(SharedString::from(line.to_owned())).with_highlights(highlights))
    });
    div()
        .flex()
        .flex_col()
        .px(px(SPACE_2))
        .py(px(SPACE_1_5))
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(pal.text_strong))
        .children(rows)
        .into_any_element()
}

fn track_hover_pointer(shell: WeakEntity<EnvironmentShell>, anchor: Bounds<Pixels>) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |card: Bounds<Pixels>, (), window, _| {
            let text = anchor.dilate(px(KEEP_MARGIN_PX));
            let shell = shell.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                let zone = if card.contains(&event.position) {
                    PointerZone::Card
                } else if text.contains(&event.position) {
                    PointerZone::Text
                } else {
                    PointerZone::Elsewhere
                };
                shell
                    .update(cx, |shell, cx| shell.hover_pointer_moved(zone, cx))
                    .unwrap_or_default();
            });
        },
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

impl EnvironmentShell {
    fn syntax_named(&self, language: Option<&str>, fallback: Syntax) -> Syntax {
        language
            .and_then(|language| self.language_catalog().get(&LanguageId::new(language)))
            .map_or(fallback, |language| Syntax::from(Some(language)))
    }

    pub(in crate::environment_shell) fn show_hover(
        &mut self,
        document: &Document,
        offset: usize,
        hover: Option<Hover>,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.editor_popover, Some(EditorPopover::Actions(_))) {
            return;
        }
        let Some(tab) = self.tab_showing(document) else {
            return;
        };
        let Some((state, _)) = self.popover_editor(tab) else {
            return;
        };
        let problems = self
            .envs
            .get(&self.active)
            .and_then(|live| live.editors.get(&tab))
            .and_then(|editor| editor.problems.clone())
            .map_or_else(Vec::new, |problems| problems.read(cx).at(offset));
        let blocks = hover.as_ref().map_or_else(Vec::new, hover_blocks);
        let has_content = !blocks.is_empty() || !problems.is_empty();
        if let Some(EditorPopover::Hover(card)) = &self.editor_popover {
            if !card.pointer.takes_new_hover(has_content) {
                return;
            }
        }
        if !has_content {
            self.close_hover(cx);
            return;
        }
        let text = state.read(cx).text();
        let anchor = hover
            .and_then(|hover| hover.range)
            .map(|range| text.position_to_offset(&range.start)..text.position_to_offset(&range.end))
            .or_else(|| problems.first().map(|problem| problem.range.clone()))
            .unwrap_or_else(|| word_around(text, offset));
        let syntax = Syntax::from(self.language_of(&document.path).as_ref());
        let scrolled_at = state.read(cx).scroll_offset();
        self.editor_popover = Some(EditorPopover::Hover(HoverCard {
            tab,
            anchor,
            scrolled_at,
            blocks,
            problems,
            syntax,
            pointer: HoverPointer::OnText,
        }));
        cx.notify();
    }

    fn hover_pointer_moved(&mut self, zone: PointerZone, cx: &mut Context<Self>) {
        let Some(EditorPopover::Hover(card)) = &mut self.editor_popover else {
            return;
        };
        match card.pointer.moved(zone, Instant::now()) {
            PointerStep::Stay => {}
            PointerStep::Leave => Self::close_hover_after_grace(cx),
            PointerStep::Close => self.close_hover(cx),
        }
    }

    fn close_hover_after_grace(cx: &mut Context<Self>) {
        let timer = cx.background_executor().timer(HOVER_GRACE);
        cx.spawn(async move |this, cx| {
            timer.await;
            this.update(cx, |shell, cx| {
                let left = matches!(
                    &shell.editor_popover,
                    Some(EditorPopover::Hover(card)) if card.pointer.grace_elapsed(Instant::now())
                );
                if left {
                    shell.close_hover(cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn close_hover(&mut self, cx: &mut Context<Self>) {
        if matches!(self.editor_popover, Some(EditorPopover::Hover(_))) {
            self.editor_popover = None;
            cx.notify();
        }
    }

    fn show_fixes(
        &mut self,
        state: &Entity<EditorState>,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_hover(cx);
        state.update(cx, |state, cx| {
            let position: Position = state.text().offset_to_position(at);
            state.set_cursor_position(position, window, cx);
            state.focus(window, cx);
        });
        window.dispatch_action(Box::new(ToggleCodeActions), cx);
    }

    fn hover_problem_row(
        index: usize,
        problem: &EditorProblem,
        state: &Entity<EditorState>,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (glyph, color) = severity_look(problem.severity, pal);
        let at = problem.range.start;
        let state = state.clone();
        div()
            .flex()
            .flex_row()
            .items_start()
            .gap(px(SPACE_2))
            .px(px(SPACE_2))
            .py(px(SPACE_1))
            .child(
                div()
                    .pt(px(SPACE_1 / 2.0))
                    .child(icon(glyph, ICON_SM, color)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_strong))
                    .child(problem.message.clone())
                    .children(problem.source.clone().map(|source| {
                        div()
                            .text_size(px(TEXT_XS))
                            .line_height(px(LINE_XS))
                            .text_color(rgb(pal.text_muted))
                            .child(source)
                    })),
            )
            .child(
                Button::new((FIXES_PREFIX, u64::try_from(index).unwrap_or(u64::MAX)))
                    .label(FIXES_LABEL)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.show_fixes(&state, at, window, cx);
                    })),
            )
            .into_any_element()
    }

    pub(super) fn hover_overlay(
        &self,
        card: &HoverCard,
        state: &Entity<EditorState>,
        root: (f32, f32),
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if state.read(cx).scroll_offset() != card.scrolled_at {
            return None;
        }
        let anchor = Self::anchor_bounds(state, &card.anchor, cx)?;
        let pal = palette(cx);
        let placement = place(anchor, self.frame_inset, root, (HOVER_W, HOVER_MAX_H));
        let problems: Vec<AnyElement> = card
            .problems
            .iter()
            .enumerate()
            .map(|(index, problem)| Self::hover_problem_row(index, problem, state, pal, cx))
            .collect();
        let divider = (!card.problems.is_empty() && !card.blocks.is_empty())
            .then(|| div().my(px(SPACE_1)).h(px(1.0)).bg(pal.divider));
        let blocks: Vec<AnyElement> = card
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| match block {
                HoverBlock::Code { language, text } => {
                    let syntax = self.syntax_named(language.as_deref(), card.syntax);
                    code_block(text, syntax, pal, cx)
                }
                HoverBlock::Text(text) => div()
                    .px(px(SPACE_2))
                    .py(px(SPACE_1))
                    .text_size(px(TEXT_SM))
                    .child(
                        TextView::markdown(
                            ElementId::from((
                                MARKDOWN_PREFIX,
                                u64::try_from(index).unwrap_or(u64::MAX),
                            )),
                            SharedString::from(text.clone()),
                        )
                        .selectable(true),
                    )
                    .into_any_element(),
            })
            .collect();
        let shell = cx.entity().downgrade();
        Some(
            placed(popover_panel(pal, cx), placement)
                .id(HOVER_ID)
                .w(px(HOVER_W))
                .overflow_y_scroll()
                .occlude()
                .font_weight(FontWeight::NORMAL)
                .child(track_hover_pointer(shell, anchor))
                .children(problems)
                .children(divider)
                .children(blocks)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_under_the_pointer_is_the_hovers_anchor() {
        let text = Rope::from("let pluginLoader = load();\n");
        assert_eq!(word_around(&text, 7), 4..16);
        assert_eq!(word_around(&text, 4), 4..16);
        assert_eq!(word_around(&text, 16), 4..16);
        assert_eq!(word_around(&text, 17), 17..17);
    }
}
