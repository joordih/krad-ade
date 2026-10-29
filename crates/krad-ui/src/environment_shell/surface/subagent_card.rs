use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_chat::{Speaker, ToolCall};
use krad_domain::{AgentState, TerminalId, ToolUseId};
use std::time::Instant;

use super::super::chat::{question_bubble, AnswerPart};
use super::super::EnvironmentShell;
use super::subagents::SubagentRun;
use super::view::{column, muted_line, status_line};
use super::{SurfaceChat, WORKING_LABEL};
use crate::components::{agent_status, icon, IconName, StatusKind};
use crate::theme::{
    palette, Palette, ICON_SM, LINE_SM, LINE_XS, RADIUS_MD, SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM,
    TEXT_XS, VIEW_HEADER_H,
};

const CARD_PREFIX: &str = "surface-subagent";
const BACK_PREFIX: &str = "surface-subagent-back";
const THREAD_PREFIX: &str = "surface-subagent-thread";
const AGENT_LABEL: &str = "Agent";
const STARTING_LABEL: &str = "Starting…";
const BACK_LABEL: &str = "Back";
const TOOLS_SUFFIX: &str = "tools";
const ONE_TOOL_SUFFIX: &str = "tool";
const HEADER_SEPARATOR: &str = "·";
const THREAD_PAD_Y: f32 = 20.0;
const MESSAGE_GAP: f32 = 22.0;

fn last_call(run: &SubagentRun) -> Option<&ToolCall> {
    run.messages
        .iter()
        .flat_map(|message| &message.parts)
        .rev()
        .find_map(|part| match part {
            AnswerPart::Tool(call) => Some(call),
            AnswerPart::Thinking(_) | AnswerPart::Text(_) | AnswerPart::Ask(_) => None,
        })
}

fn tool_count_label(count: usize) -> String {
    let suffix = if count == 1 {
        ONE_TOOL_SUFFIX
    } else {
        TOOLS_SUFFIX
    };
    format!("{count} {suffix}")
}

fn kind_and_task(call: Option<&ToolCall>, run: Option<&SubagentRun>) -> (String, String) {
    let kind = run
        .and_then(|run| run.meta.agent_type.clone())
        .unwrap_or_else(|| AGENT_LABEL.to_owned());
    let task = run
        .and_then(|run| run.meta.description.clone())
        .or_else(|| call.map(|call| call.summary.clone()))
        .unwrap_or_default();
    (kind, task)
}

fn status_mark(finished: bool, pal: &Palette) -> AnyElement {
    if finished {
        icon(IconName::Tick, ICON_SM, pal.text_muted).into_any_element()
    } else {
        agent_status(StatusKind::Agent(AgentState::Working), ICON_SM, pal).into_any_element()
    }
}

impl EnvironmentShell {
    pub(super) fn subagent_card(
        id: TerminalId,
        call: &ToolCall,
        tool: &ToolUseId,
        run: Option<&SubagentRun>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let finished = run.is_some_and(|run| run.meta.finished);
        let (kind, task) = kind_and_task(Some(call), run);
        let count = run.map(SubagentRun::tool_count).filter(|count| *count > 0);
        let activity = match run {
            None => Some(STARTING_LABEL.to_owned()),
            Some(run) if !finished => {
                last_call(run).map(|call| format!("{} {}", call.name, call.summary))
            }
            Some(_) => None,
        };
        let entered = tool.clone();
        div()
            .id(ElementId::Name(SharedString::from(format!(
                "{CARD_PREFIX}-{}-{}",
                id.as_u64(),
                tool.as_str()
            ))))
            .flex()
            .flex_col()
            .gap(px(SPACE_1_5))
            .px(px(SPACE_3))
            .py(px(SPACE_2))
            .rounded(px(RADIUS_MD))
            .bg(rgb(pal.bg_panel))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.enter_subagent(id, &entered, cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .child(icon(IconName::Workflow, ICON_SM, pal.text_muted))
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(pal.text_strong))
                            .child(kind),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(pal.text_muted))
                            .child(task),
                    )
                    .children(count.map(|count| {
                        div()
                            .flex_none()
                            .text_size(px(TEXT_XS))
                            .text_color(rgb(pal.text_muted))
                            .child(tool_count_label(count))
                    }))
                    .child(status_mark(finished, pal))
                    .child(icon(IconName::ChevronRight, ICON_SM, pal.text_muted)),
            )
            .children(activity.map(|activity| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(activity)
            }))
            .into_any_element()
    }

    pub(super) fn subagent_view(
        id: TerminalId,
        tool: &ToolUseId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let run = chat.subagents.runs.get(tool);
        let finished = run.is_some_and(|run| run.meta.finished);
        let mut thread: Vec<AnyElement> = Vec::new();
        for (index, message) in run.map_or(&[][..], |run| &run.messages).iter().enumerate() {
            thread.push(match message.speaker {
                Speaker::User => question_bubble(&message.text, false, pal),
                Speaker::Assistant => Self::surface_answer(id, index, message, chat, cx),
            });
        }
        if run.is_none() {
            thread.push(muted_line(STARTING_LABEL, pal));
        }
        if !finished {
            let started = chat.working_since.get().unwrap_or_else(Instant::now);
            chat.working_since.set(Some(started));
            let mono = cx.theme().mono_font_family.clone();
            thread.push(status_line(
                id.as_u64(),
                WORKING_LABEL,
                started,
                None,
                mono,
                pal,
            ));
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .font_family(cx.theme().font_family.clone())
            .child(Self::subagent_header(id, run, finished, cx))
            .child(
                div()
                    .id((THREAD_PREFIX, id.as_u64()))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&chat.subagents.scroll)
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        column()
                            .py(px(THREAD_PAD_Y))
                            .gap(px(MESSAGE_GAP))
                            .children(thread),
                    ),
            )
            .into_any_element()
    }

    fn subagent_header(
        id: TerminalId,
        run: Option<&SubagentRun>,
        finished: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let (kind, task) = kind_and_task(None, run);
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(VIEW_HEADER_H))
            .px(px(SPACE_3))
            .border_b_1()
            .border_color(pal.divider)
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .child(
                div()
                    .id((BACK_PREFIX, id.as_u64()))
                    .flex()
                    .flex_row()
                    .flex_none()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .px(px(SPACE_2))
                    .py(px(SPACE_1_5))
                    .rounded(px(RADIUS_MD))
                    .cursor_pointer()
                    .text_color(rgb(pal.text_default))
                    .hover(|style| style.bg(pal.bg_hover))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.leave_subagent(id, cx);
                    }))
                    .child(icon(IconName::ChevronLeft, ICON_SM, pal.text_default))
                    .child(BACK_LABEL),
            )
            .child(icon(IconName::Workflow, ICON_SM, pal.text_muted))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_strong))
                    .child(kind),
            )
            .child(
                div()
                    .text_color(rgb(pal.text_muted))
                    .child(HEADER_SEPARATOR),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_muted))
                    .child(task),
            )
            .child(status_mark(finished, pal))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tool_count_reads_in_the_singular_for_one() {
        assert_eq!(tool_count_label(1), "1 tool");
        assert_eq!(tool_count_label(4), "4 tools");
    }
}
