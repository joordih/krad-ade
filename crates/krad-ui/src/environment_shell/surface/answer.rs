use gpui_kit::base::text::CodeBlock;
use gpui_kit::component::text::TextView;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, App, Context, ElementId, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_chat::ToolCall;
use krad_domain::{TerminalId, ToolKind};
use std::borrow::Cow;

use super::super::ask::ask_line;
use super::super::chat::{AnswerPart, ChatMessage};
use super::super::EnvironmentShell;
use super::message_actions::{copy_button, message_actions, message_group};
use super::tool_group::ToolLook;
use super::view::error_line;
use super::{SurfaceChat, THINKING_LABEL};
use crate::components::{icon, IconName};
use crate::theme::{palette, ICON_XS, LINE_SM, LINE_XS, SPACE_1_5, SPACE_3, TEXT_BASE, TEXT_XS};

const PART_GAP: f32 = 12.0;
const ANSWER_LINE_H: f32 = 22.0;
const THOUGHT_RULE_W: f32 = 2.0;
const THOUGHT_PREFIX: &str = "surface-thought";
const ANSWER_PREFIX: &str = "surface-answer";
const CODE_COPY_PREFIX: &str = "surface-code-copy";
const CODE_COPY_TOOLTIP: &str = "Copy code";
const MESSAGE_COPY_PREFIX: &str = "surface-answer-copy";
const MESSAGE_COPY_TOOLTIP: &str = "Copy message";
const PARAGRAPH_BREAK: &str = "\n\n";
const RUN_QUESTION: &str = "Allow this command?";
const READ_QUESTION: &str = "Allow reading this file?";
const EDIT_QUESTION: &str = "Allow this edit?";
const SEARCH_QUESTION: &str = "Allow this search?";
const SUBAGENT_QUESTION: &str = "Allow this subagent?";
const PLAN_QUESTION: &str = "Allow this plan update?";
const FETCH_QUESTION: &str = "Allow this request?";
const USE_QUESTION: &str = "Allow";

pub(in crate::environment_shell) fn tool_question(name: &str, kind: ToolKind) -> Cow<'static, str> {
    Cow::Borrowed(match ToolLook::from(kind) {
        ToolLook::Shell => RUN_QUESTION,
        ToolLook::Read => READ_QUESTION,
        ToolLook::Edit => EDIT_QUESTION,
        ToolLook::Search => SEARCH_QUESTION,
        ToolLook::Agent => SUBAGENT_QUESTION,
        ToolLook::Plan => PLAN_QUESTION,
        ToolLook::Web => FETCH_QUESTION,
        ToolLook::Other => return Cow::Owned(format!("{USE_QUESTION} {name}?")),
    })
}

fn answer_text(message: &ChatMessage) -> String {
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            AnswerPart::Text(text) => Some(text.trim()),
            AnswerPart::Thinking(_) | AnswerPart::Tool(_) | AnswerPart::Ask(_) => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(PARAGRAPH_BREAK)
}

fn code_copy_button(block: &CodeBlock, _: &mut Window, cx: &mut App) -> AnyElement {
    let start = block.span.as_ref().map_or(0, |span| span.start);
    copy_button(
        ElementId::Name(format!("{CODE_COPY_PREFIX}-{start}").into()),
        block.code().to_string(),
        CODE_COPY_TOOLTIP,
        palette(cx),
    )
}

impl EnvironmentShell {
    pub(super) fn surface_answer(
        id: TerminalId,
        index: usize,
        message: &ChatMessage,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let raw = id.as_u64();
        let mut parts: Vec<AnyElement> = Vec::new();
        let mut calls: Vec<(usize, &ToolCall)> = Vec::new();
        let group = |calls: &mut Vec<(usize, &ToolCall)>,
                     running: bool,
                     parts: &mut Vec<AnyElement>,
                     cx: &mut Context<Self>| {
            let Some(&(start, _)) = calls.first() else {
                return;
            };
            let key = (index, start);
            let open = chat.tool_groups.get(&key).copied();
            parts.push(Self::tool_group(id, key, calls, running, open, &mono, cx));
            calls.clear();
        };
        for (part_index, part) in message.parts.iter().enumerate() {
            if let AnswerPart::Tool(call) = part {
                match call.id.as_ref().filter(|_| call.kind == ToolKind::Subagent) {
                    Some(tool) => {
                        group(&mut calls, false, &mut parts, cx);
                        let run = chat.subagents.runs.get(tool);
                        parts.push(Self::subagent_card(id, call, tool, run, cx));
                    }
                    None => calls.push((part_index, call)),
                }
                continue;
            }
            group(&mut calls, false, &mut parts, cx);
            match part {
                AnswerPart::Thinking(thought) => {
                    let opened = chat.opened_thoughts.contains(&(index, part_index));
                    parts.push(Self::thought(id, (index, part_index), thought, opened, cx));
                }
                AnswerPart::Text(text) => {
                    let answer_id = ElementId::Name(SharedString::from(format!(
                        "{ANSWER_PREFIX}-{raw}-{index}-{part_index}"
                    )));
                    parts.push(
                        div()
                            .text_size(px(TEXT_BASE))
                            .line_height(px(ANSWER_LINE_H))
                            .text_color(rgb(pal.text_strong))
                            .child(
                                TextView::markdown(answer_id, text.clone())
                                    .selectable(true)
                                    .code_block_actions(code_copy_button),
                            )
                            .into_any_element(),
                    );
                }
                AnswerPart::Ask(outcome) => parts.push(ask_line(outcome, pal)),
                AnswerPart::Tool(_) => {}
            }
        }
        group(&mut calls, message.running, &mut parts, cx);
        parts.extend(message.error.as_deref().map(|error| error_line(error, pal)));
        let said = answer_text(message);
        let group = message_group(id, index);
        if !message.running && !said.is_empty() {
            let key = format!("{MESSAGE_COPY_PREFIX}-{raw}-{index}");
            let copy = copy_button(ElementId::Name(key.into()), said, MESSAGE_COPY_TOOLTIP, pal);
            parts.push(message_actions(group.clone(), [copy]));
        }
        div()
            .group(group)
            .flex()
            .flex_col()
            .gap(px(PART_GAP))
            .children(parts)
            .into_any_element()
    }

    fn thought(
        id: TerminalId,
        key: (usize, usize),
        thought: &str,
        opened: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let row_id = SharedString::from(format!(
            "{THOUGHT_PREFIX}-{}-{}-{}",
            id.as_u64(),
            key.0,
            key.1
        ));
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1_5))
            .child(
                div()
                    .id(ElementId::Name(row_id))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .cursor_pointer()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .hover(|style| style.text_color(rgb(pal.text_default)))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.toggle_thought(id, key, cx);
                    }))
                    .child(icon(IconName::Magic, ICON_XS, pal.text_muted))
                    .child(THINKING_LABEL)
                    .child(icon(
                        if opened {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        },
                        ICON_XS,
                        pal.text_muted,
                    )),
            )
            .when(opened, |block| {
                block.child(
                    div()
                        .pl(px(SPACE_3))
                        .border_l(px(THOUGHT_RULE_W))
                        .border_color(pal.divider)
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_SM))
                        .text_color(rgb(pal.text_muted))
                        .child(thought.to_owned()),
                )
            })
            .into_any_element()
    }

    fn toggle_thought(&mut self, id: TerminalId, key: (usize, usize), cx: &mut Context<Self>) {
        let env = self.active.clone();
        if let Some(chat) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            if !chat.opened_thoughts.remove(&key) {
                chat.opened_thoughts.insert(key);
            }
            chat.thread.remeasure();
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ask_says_what_it_asks_for() {
        assert_eq!(tool_question("Bash", ToolKind::Shell), RUN_QUESTION);
        assert_eq!(tool_question("Write", ToolKind::Edit), EDIT_QUESTION);
        assert_eq!(tool_question("Grep", ToolKind::Search), SEARCH_QUESTION);
        assert_eq!(
            tool_question("mcp__linear__create", ToolKind::Other),
            "Allow mcp__linear__create?"
        );
    }

    #[test]
    fn copying_a_reply_takes_only_what_the_agent_said() {
        let message = ChatMessage {
            speaker: krad_chat::Speaker::Assistant,
            text: String::new(),
            parts: vec![
                AnswerPart::Thinking("weigh it".to_owned()),
                AnswerPart::Text(" First part. ".to_owned()),
                AnswerPart::Tool(ToolCall {
                    id: None,
                    name: "Bash".to_owned(),
                    kind: ToolKind::Shell,
                    summary: "ls".to_owned(),
                    edit: None,
                }),
                AnswerPart::Text("Second part.".to_owned()),
            ],
            error: None,
            running: false,
            mentions: Vec::new(),
            checklist: None,
            rewind: None,
        };
        assert_eq!(answer_text(&message), "First part.\n\nSecond part.");
    }
}
