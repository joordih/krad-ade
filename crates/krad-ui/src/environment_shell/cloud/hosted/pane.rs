use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{AskAnswer, AskKind, ChatMessage, PermissionMode, Prompt, RewindScope, Speaker};
use krad_protocol::{ChatAsk, ChatCommand, ChatMirror};

use super::super::super::EnvironmentShell;
use super::HostedChatKey;
use crate::components::{card, Button, ButtonVariant};
use crate::theme::{
    palette, Palette, LINE_SM, LINE_XS, RADIUS_XS, SPACE_1, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};

const THREAD_ID: &str = "hosted-chat-thread";
const STOP_ID: &str = "hosted-chat-stop";
const CLOSE_ID: &str = "hosted-chat-close";
const SEND_ID: &str = "hosted-chat-send";
const ALLOW_PREFIX: &str = "hosted-ask-allow";
const DENY_PREFIX: &str = "hosted-ask-deny";
const REWIND_PREFIX: &str = "hosted-rewind";
const STOP_LABEL: &str = "Stop";
const CLOSE_LABEL: &str = "Close";
const SEND_LABEL: &str = "Send";
const ALLOW_LABEL: &str = "Allow";
const DENY_LABEL: &str = "Deny";
const APPROVE_LABEL: &str = "Approve plan";
const REWIND_LABEL: &str = "Rewind here";
const YOU: &str = "You";
const AGENT: &str = "Agent";
const WORKING: &str = "Working…";
const GONE: &str = "This cloud chat is no longer on the runner";
const QUESTIONS: &str = "The agent asks questions; answer them in a full client";
const PLAN: &str = "The agent proposes a plan";
const CAPTION_SEPARATOR: &str = " · ";

fn ask_summary(kind: &AskKind) -> String {
    match kind {
        AskKind::Permission { summary, title, .. } => {
            title.clone().unwrap_or_else(|| summary.clone())
        }
        AskKind::AcpPermission { tool, .. } => tool.summary.clone(),
        AskKind::Plan { .. } => PLAN.to_owned(),
        AskKind::Questions(_) => QUESTIONS.to_owned(),
    }
}

fn allow_answer(kind: &AskKind) -> Option<(AskAnswer, &'static str)> {
    match kind {
        AskKind::Permission { .. } | AskKind::AcpPermission { .. } => {
            Some((AskAnswer::Allow, ALLOW_LABEL))
        }
        AskKind::Plan { .. } => Some((
            AskAnswer::ApprovePlan {
                mode: PermissionMode::Manual,
            },
            APPROVE_LABEL,
        )),
        AskKind::Questions(_) => None,
    }
}

fn header_caption(mirror: &ChatMirror, runner: &str) -> String {
    let state = &mirror.state;
    [
        Some(runner.to_owned()),
        state.model.clone(),
        state.effort.map(|effort| effort.to_string()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(CAPTION_SEPARATOR)
}

fn bubble(message: &ChatMessage, pal: &Palette) -> gpui_kit::Div {
    let you = message.speaker == Speaker::User;
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_1))
        .p(px(SPACE_2))
        .rounded(px(RADIUS_XS))
        .when(you, |bubble| bubble.bg(pal.bg_selected))
        .child(
            div()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_muted))
                .child(if you { YOU } else { AGENT }),
        )
        .child(
            div()
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(pal.text_strong))
                .child(message.text.clone()),
        )
        .children(message.error.clone().map(|error| {
            div()
                .text_size(px(TEXT_XS))
                .text_color(rgb(pal.error_fg))
                .child(error)
        }))
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn hosted_chat_pane(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let key = self.cloud.shown.clone()?;
        let pal = palette(cx);
        let mirror = self
            .runtime
            .view
            .read(cx)
            .remotes
            .chat(key.runner, &key.chat)
            .map(|chat| chat.mirror.clone());
        let Some(mirror) = mirror else {
            return Some(
                card(pal, cx)
                    .size_full()
                    .p(px(SPACE_3))
                    .text_color(rgb(pal.text_muted))
                    .child(GONE)
                    .into_any_element(),
            );
        };
        let caption = header_caption(&mirror, &self.runner_name(key.runner));
        let messages = Self::hosted_messages(&key, &mirror, cx);
        let asks: Vec<AnyElement> = mirror
            .asks
            .iter()
            .map(|ask| Self::hosted_ask(&key, ask, cx))
            .collect();
        let composer = self
            .cloud
            .composer
            .as_ref()
            .filter(|composer| composer.key == key)
            .map(|composer| Textarea::new(&composer.input));
        Some(
            card(pal, cx)
                .size_full()
                .flex()
                .flex_col()
                .gap(px(SPACE_2))
                .p(px(SPACE_3))
                .child(self.hosted_header(&key, &mirror, caption, cx))
                .child(
                    div()
                        .id(THREAD_ID)
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .gap(px(SPACE_2))
                        .overflow_y_scroll()
                        .children(messages)
                        .when(mirror.state.working, |thread| {
                            thread.child(
                                div()
                                    .text_size(px(TEXT_XS))
                                    .text_color(rgb(pal.text_muted))
                                    .child(WORKING),
                            )
                        }),
                )
                .children(asks)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_end()
                        .gap(px(SPACE_2))
                        .child(div().flex_1().min_w_0().children(composer))
                        .child(
                            Button::new(SEND_ID)
                                .label(SEND_LABEL)
                                .variant(ButtonVariant::Primary)
                                .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                                    this.send_hosted(window, cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    fn hosted_header(
        &self,
        key: &HostedChatKey,
        mirror: &ChatMirror,
        caption: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let title = self.settings.acp.harness_name(mirror.state.harness);
        let stop_key = key.clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(pal.text_strong))
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(TEXT_XS))
                            .text_color(rgb(pal.text_muted))
                            .child(caption),
                    ),
            )
            .when(mirror.state.working, |header| {
                header.child(
                    Button::new(STOP_ID)
                        .label(STOP_LABEL)
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            let chat = stop_key.chat.clone();
                            this.post_hosted(stop_key.runner, ChatCommand::Interrupt { chat }, cx);
                        })),
                )
            })
            .child(
                Button::new(CLOSE_ID)
                    .label(CLOSE_LABEL)
                    .variant(ButtonVariant::Ghost)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.leave_hosted_chat();
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn hosted_messages(
        key: &HostedChatKey,
        mirror: &ChatMirror,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pal = palette(cx);
        mirror
            .messages
            .iter()
            .enumerate()
            .map(|(local, message)| {
                let index = mirror.from + local;
                let rewind = (message.speaker == Speaker::User && !mirror.state.working)
                    .then(|| Self::rewind_button(key, index, &message.text, cx));
                bubble(message, pal).children(rewind).into_any_element()
            })
            .collect()
    }

    fn rewind_button(
        key: &HostedChatKey,
        index: usize,
        text: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = key.clone();
        let text = text.to_owned();
        div()
            .id((REWIND_PREFIX, index as u64))
            .child(
                Button::new((REWIND_PREFIX, index as u64))
                    .label(REWIND_LABEL)
                    .variant(ButtonVariant::Ghost)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        let rewind = ChatCommand::Rewind {
                            chat: key.chat.clone(),
                            index,
                            prompt: Prompt::from(text.as_str()),
                            scope: RewindScope::Conversation,
                        };
                        this.post_hosted(key.runner, rewind, cx);
                    })),
            )
            .into_any_element()
    }

    fn hosted_ask(key: &HostedChatKey, ask: &ChatAsk, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let raw = ask.ask.0;
        let answer = |answer: AskAnswer| {
            let key = key.clone();
            let id = ask.ask;
            cx.listener(move |this: &mut Self, _, _, cx| {
                let command = ChatCommand::AnswerAsk {
                    chat: key.chat.clone(),
                    ask: id,
                    answer: answer.clone(),
                };
                this.post_hosted(key.runner, command, cx);
            })
        };
        let allow = allow_answer(&ask.kind).map(|(allowed, label)| {
            Button::new((ALLOW_PREFIX, raw))
                .label(label)
                .variant(ButtonVariant::Primary)
                .on_click(answer(allowed))
        });
        let deny = Button::new((DENY_PREFIX, raw))
            .label(DENY_LABEL)
            .variant(ButtonVariant::Secondary)
            .on_click(answer(AskAnswer::Deny {
                message: String::new(),
            }));
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .p(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .bg(pal.bg_selected)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(TEXT_SM))
                    .text_color(rgb(pal.attention_text))
                    .child(ask_summary(&ask.kind)),
            )
            .children(allow)
            .child(deny)
            .into_any_element()
    }
}
