use gpui_kit::component::text::TextView;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use krad_chat::ToolCall;
use krad_domain::TerminalId;

use super::super::ask::ask_line;
use super::super::chat::{AnswerPart, ChatMessage};
use super::super::EnvironmentShell;
use super::tool_group::ToolKind;
use super::view::error_line;
use super::{SurfaceChat, THINKING_LABEL};
use crate::components::{icon, IconName};
use crate::theme::{palette, ICON_XS, LINE_SM, LINE_XS, SPACE_1_5, SPACE_3, TEXT_BASE, TEXT_XS};

const PART_GAP: f32 = 12.0;
const ANSWER_LINE_H: f32 = 22.0;
const THOUGHT_RULE_W: f32 = 2.0;
const THOUGHT_PREFIX: &str = "surface-thought";
const ANSWER_PREFIX: &str = "surface-answer";

pub(in crate::environment_shell) fn tool_icon(name: &str) -> IconName {
    ToolKind::of(name).icon()
}

pub(super) fn is_agent_tool(name: &str) -> bool {
    ToolKind::of(name) == ToolKind::Agent
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
                match call.id.as_ref().filter(|_| is_agent_tool(&call.name)) {
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
                            .child(TextView::markdown(answer_id, text.clone()).selectable(true))
                            .into_any_element(),
                    );
                }
                AnswerPart::Ask(outcome) => parts.push(ask_line(outcome, pal)),
                AnswerPart::Tool(_) => {}
            }
        }
        group(&mut calls, message.running, &mut parts, cx);
        parts.extend(message.error.as_deref().map(|error| error_line(error, pal)));
        div()
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
    fn tools_get_the_icon_of_what_they_touch() {
        assert_eq!(tool_icon("Bash"), IconName::Terminal);
        assert_eq!(tool_icon("Edit"), IconName::File);
        assert_eq!(tool_icon("Grep"), IconName::Search);
        assert_eq!(tool_icon("mcp__linear__create"), IconName::Plug);
    }
}
