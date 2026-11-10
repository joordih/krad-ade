use gpui_kit::base::text::CodeBlock;
use gpui_kit::component::text::TextView;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rems, rgb, AnyElement, Context, ElementId, Entity, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_chat::{ToolCall, ToolUseId};
use krad_domain::{TerminalId, ToolKind};
use std::borrow::Cow;

use super::super::ask::ask_line;
use super::super::chat::{AnswerPart, ChatMessage};
use super::super::workers::{spawned_worker, WorkerCard};
use super::super::EnvironmentShell;
use super::answer_blocks::{answer_blocks, AnswerBlock};
use super::chat_menu::{ChatSpot, CodeSpot};
use super::message_actions::{copy_button, message_actions, message_group, BookmarkAction};
use super::prose_views::ProseKey;
use super::tool_group::{ToolLook, ToolRows};
use super::view::error_line;
use super::{SurfaceChat, THINKING_LABEL};
use crate::components::{icon, CopyFeedback, IconName};
use crate::theme::{palette, ICON_XS, LINE_SM, LINE_XS, SPACE_1_5, SPACE_3, TEXT_BASE, TEXT_XS};

const PART_GAP: f32 = 12.0;
const BLOCK_GAP_REMS: f32 = 0.75;
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
const RUN_REQUEST: &str = "Allow running";
const READ_REQUEST: &str = "Allow reading";
const EDIT_REQUEST: &str = "Allow edit to";
const SEARCH_REQUEST: &str = "Allow searching";
const SUBAGENT_REQUEST: &str = "Allow subagent";
const PLAN_REQUEST: &str = "Allow plan update";
const FETCH_REQUEST: &str = "Allow request to";

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

pub(in crate::environment_shell) fn tool_request(name: &str, kind: ToolKind) -> Cow<'static, str> {
    Cow::Borrowed(match ToolLook::from(kind) {
        ToolLook::Shell => RUN_REQUEST,
        ToolLook::Read => READ_REQUEST,
        ToolLook::Edit => EDIT_REQUEST,
        ToolLook::Search => SEARCH_REQUEST,
        ToolLook::Agent => SUBAGENT_REQUEST,
        ToolLook::Plan => PLAN_REQUEST,
        ToolLook::Web => FETCH_REQUEST,
        ToolLook::Other => return Cow::Owned(format!("{USE_QUESTION} {name}")),
    })
}

pub(in crate::environment_shell) fn names_its_tool(kind: ToolKind) -> bool {
    ToolLook::from(kind) == ToolLook::Other
}

pub(super) fn answer_text(message: &ChatMessage) -> String {
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

#[derive(Clone, Copy)]
pub(super) enum AnswerPlace<'a> {
    Thread,
    Subagent(&'a ToolUseId),
}

#[derive(Clone, Copy)]
pub(super) struct ShownAnswer<'a> {
    pub(super) id: TerminalId,
    pub(super) index: usize,
    pub(super) message: &'a ChatMessage,
    pub(super) place: AnswerPlace<'a>,
}

impl ShownAnswer<'_> {
    fn key(&self, prefix: &str, suffix: &str) -> SharedString {
        let raw = self.id.as_u64();
        let index = self.index;
        SharedString::from(match self.place {
            AnswerPlace::Thread => format!("{prefix}-{raw}-{index}{suffix}"),
            AnswerPlace::Subagent(tool) => {
                format!("{prefix}-{raw}-{}-{index}{suffix}", tool.as_str())
            }
        })
    }
}

impl EnvironmentShell {
    pub(super) fn surface_answer(
        answer: ShownAnswer<'_>,
        chat: &SurfaceChat,
        workers: &[WorkerCard],
        bookmark: Option<BookmarkAction>,
        copies: &Entity<CopyFeedback>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ShownAnswer {
            id, index, message, ..
        } = answer;
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
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
            let rows = ToolRows {
                calls,
                outputs: &message.tool_outputs,
            };
            parts.push(Self::tool_group(id, key, &rows, running, open, &mono, cx));
            calls.clear();
        };
        for (part_index, part) in message.parts.iter().enumerate() {
            if let AnswerPart::Tool(call) = part {
                let spawned = (call.kind == ToolKind::Shell)
                    .then(|| spawned_worker(&call.summary, workers))
                    .flatten();
                if let Some(card) = spawned {
                    group(&mut calls, false, &mut parts, cx);
                    parts.push(Self::worker_card(id, card, cx));
                    continue;
                }
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
                    parts.push(Self::answer_prose(
                        answer, part_index, text, chat, copies, cx,
                    ));
                }
                AnswerPart::Ask(outcome) => parts.push(ask_line(outcome, pal)),
                AnswerPart::Tool(_) => {}
            }
        }
        group(&mut calls, message.running, &mut parts, cx);
        parts.extend(message.error.as_deref().map(|error| error_line(error, pal)));
        let said = answer_text(message);
        let group = message_group(id, index);
        let copy = (!message.running && !said.is_empty()).then(|| {
            let key = answer.key(MESSAGE_COPY_PREFIX, "");
            copy_button(copies, key, said, MESSAGE_COPY_TOOLTIP, cx)
        });
        let bookmark = bookmark.filter(|bookmark| bookmark.marked || copy.is_some());
        if copy.is_some() || bookmark.is_some() {
            parts.push(message_actions(group.clone(), copy, bookmark));
        }
        div()
            .group(group)
            .flex()
            .flex_col()
            .gap(px(PART_GAP))
            .children(parts)
            .into_any_element()
    }

    fn answer_prose(
        answer: ShownAnswer<'_>,
        part: usize,
        text: &str,
        chat: &SurfaceChat,
        copies: &Entity<CopyFeedback>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let scale = chat.text_scale.get();
        let mut views = chat.prose.borrow_mut();
        let blocks: Vec<AnyElement> = answer_blocks(text)
            .into_iter()
            .enumerate()
            .map(|(block, shown)| {
                let source = match shown {
                    AnswerBlock::Prose(prose) | AnswerBlock::Code { source: prose, .. } => prose,
                };
                let view = match answer.place {
                    AnswerPlace::Thread => {
                        let key = ProseKey {
                            message: answer.index,
                            part,
                            block,
                        };
                        TextView::new(&Self::prose_view(&mut views, key, source, cx))
                    }
                    AnswerPlace::Subagent(_) => {
                        let suffix = format!("-{part}-{block}");
                        let view_id = ElementId::Name(answer.key(ANSWER_PREFIX, &suffix));
                        TextView::markdown(view_id, source.to_owned())
                    }
                }
                .selectable(true);
                if matches!(shown, AnswerBlock::Prose(_)) {
                    return view.into_any_element();
                }
                let suffix = format!("-{part}-{block}");
                let copy_key = answer.key(CODE_COPY_PREFIX, &suffix);
                let copies = copies.clone();
                let code_view = view.code_block_actions(move |code: &CodeBlock, _, cx| {
                    copy_button(
                        &copies,
                        copy_key.clone(),
                        code.code().to_string(),
                        CODE_COPY_TOOLTIP,
                        cx,
                    )
                });
                let menu = match answer.place {
                    AnswerPlace::Thread => Some(ChatSpot {
                        chat: answer.id,
                        message: answer.index,
                        code: Some(CodeSpot { part, block }),
                    }),
                    AnswerPlace::Subagent(_) => None,
                };
                div()
                    .when_some(menu, |view, spot| {
                        view.on_mouse_down(
                            MouseButton::Right,
                            cx.listener(
                                move |this: &mut Self, event: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    this.open_chat_menu(spot, event, window, cx);
                                },
                            ),
                        )
                    })
                    .child(code_view)
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(rems(BLOCK_GAP_REMS))
            .text_size(px(TEXT_BASE * scale))
            .line_height(px(ANSWER_LINE_H * scale))
            .text_color(rgb(pal.text_strong))
            .children(blocks)
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
                    edits: Vec::new(),
                }),
                AnswerPart::Text("Second part.".to_owned()),
            ],
            error: None,
            running: false,
            mentions: Vec::new(),
            checklist: None,
            rewind: None,
            tool_outputs: std::collections::HashMap::new(),
        };
        assert_eq!(answer_text(&message), "First part.\n\nSecond part.");
    }
}
