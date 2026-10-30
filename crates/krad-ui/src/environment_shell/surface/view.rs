use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, relative, rgb, AnyElement, Context, ElementId, ExternalPaths, FontWeight,
    InteractiveElement, IntoElement, ListState, ParentElement, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{AgentState, TerminalId};
use std::time::Instant;

use super::super::ask::{tray, tray_button, AskOwner};
use super::super::sidebar::PanelReveal;
use super::super::EnvironmentShell;
use super::dot_field::dot_field;
use super::SurfaceChat;
use crate::components::{icon, thinking_indicator, tooltip_view, IconName};
use crate::theme::{floating_shadow, palette, Palette, ICON, LINE_XS, SPACE_2, SPACE_3, TEXT_XS};

const COLUMN_MAX_W: f32 = 760.0;
const TRAY_INSET: f32 = SPACE_3;
const COLUMN_PAD_X: f32 = 24.0;
pub(super) const ROUND_BUTTON: f32 = 30.0;
const DOWN_BUTTON_OFFSET: f32 = 12.0;
const BOTTOM_SLACK: f32 = 24.0;

const DOWN_PREFIX: &str = "surface-down";
const OPEN_TERMINAL_PREFIX: &str = "surface-open-terminal";
const TOOLTIP_DOWN: &str = "Jump to the latest";
const WAITING_TEXT: &str = "is asking you something in the terminal.";
const OPEN_TERMINAL_LABEL: &str = "Open terminal";
const EXITED_TEXT: &str = "The agent's terminal has closed.";
pub(super) const LOADING_TEXT: &str = "Reading the conversation…";

pub(super) fn following(thread: &ListState) -> bool {
    thread.is_following_tail() || thread.is_scrolled_to_end().unwrap_or(true)
}

pub(super) fn at_bottom(scroll: &ScrollHandle) -> bool {
    let max = scroll.max_offset().y.as_f32();
    scroll.offset().y.as_f32() + max <= BOTTOM_SLACK
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AgentNow {
    Idle,
    Working,
    Waiting,
    Exited,
}

pub(super) fn status_line(
    key: u64,
    label: &'static str,
    started: Instant,
    activity: Option<&str>,
    mono: SharedString,
    pal: &Palette,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_2))
        .min_w_0()
        .child(thinking_indicator(key, label.into(), started, mono, pal))
        .children(activity.map(|activity| {
            div()
                .min_w_0()
                .truncate()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_muted))
                .child(activity.to_owned())
        }))
        .into_any_element()
}

const CENTRED_LIFT: f32 = 0.38;
const QUESTION_TEXT: f32 = 22.0;
const QUESTION_LINE: f32 = 30.0;
const BANNER_GAP: f32 = 28.0;
const CHAT_QUESTION: &str = "What should we work on in krad?";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn sync_chat_landings(
        &mut self,
        now: std::time::Instant,
    ) -> bool {
        let Some(live) = self.envs.get(&self.active) else {
            return false;
        };
        let started: Vec<(TerminalId, bool)> = live
            .chats
            .iter()
            .map(|(id, chat)| (*id, !chat.messages.is_empty()))
            .collect();
        let mut moving = false;
        for (id, started) in started {
            let landing = self
                .chat_landings
                .entry(id)
                .or_insert_with(|| PanelReveal::new(started));
            if started && !landing.is_open() {
                landing.toggle(now);
            }
            moving |= landing.advance(now);
        }
        moving
    }

    pub(super) fn agent_now(&self, id: TerminalId) -> AgentNow {
        let Some(live) = self.envs.get(&self.active) else {
            return AgentNow::Exited;
        };
        if let Some(structured) = live.chats.get(&id).and_then(SurfaceChat::structured) {
            return if !structured.asks.is_empty() {
                AgentNow::Waiting
            } else if structured.working {
                AgentNow::Working
            } else {
                AgentNow::Idle
            };
        }
        let running = live
            .sessions
            .get(&id)
            .is_some_and(|session| session.state.is_running());
        if !running {
            return AgentNow::Exited;
        }
        match live
            .data
            .nodes
            .iter()
            .find(|node| node.id == id)
            .map(|node| node.state)
        {
            Some(AgentState::Working) => AgentNow::Working,
            Some(AgentState::WaitingForInput) => AgentNow::Waiting,
            Some(AgentState::Idle | AgentState::Done) | None => AgentNow::Idle,
        }
    }

    pub(in crate::environment_shell) fn surface_chat_body(
        &self,
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(tool) = chat.subagents.trail.last() {
            return Self::subagent_view(id, tool, chat, cx);
        }
        let now = self.agent_now(id);
        if now != AgentNow::Working {
            chat.working_since.set(None);
        }
        let following = following(&chat.thread);
        let landed = self
            .chat_landings
            .get(&id)
            .map_or(1.0, PanelReveal::fraction);
        let pal = palette(cx);
        let hole = (landed < 1.0).then(|| self.arcade_area()).flatten();
        let banner = (landed < 1.0).then(|| {
            div()
                .w_full()
                .pb(px(BANNER_GAP))
                .opacity(1.0 - landed)
                .child(self.arcade_banner(cx))
        });
        let question = (landed < 1.0).then(|| {
            div()
                .w_full()
                .pb(px(SPACE_3))
                .opacity(1.0 - landed)
                .text_size(px(QUESTION_TEXT))
                .line_height(px(QUESTION_LINE))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(pal.text_strong))
                .child(CHAT_QUESTION)
        });
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .font_family(cx.theme().font_family.clone())
            .when(chat.structured().is_some(), |view| {
                view.on_drop(
                    cx.listener(move |this: &mut Self, dropped: &ExternalPaths, _, cx| {
                        this.attach_paths(id, dropped.paths(), cx);
                    }),
                )
            })
            .child(dot_field(pal, hole))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .opacity(landed)
                    .child(Self::surface_thread(id, chat, now, cx))
                    .when(!following, |area| {
                        area.child(Self::jump_down(id, chat.thread.clone(), cx))
                    }),
            )
            .child(
                div().flex().flex_col().flex_none().items_center().child(
                    column()
                        .pb(px(SPACE_3))
                        .children(banner)
                        .children(question)
                        .child(
                            div()
                                .relative()
                                .w_full()
                                .children(
                                    self.ask_tray(AskOwner::Surface(id), cx)
                                        .or_else(|| self.mcp_tray(id, chat, cx))
                                        .or_else(|| self.agent_notice(id, chat, cx))
                                        .or_else(|| Self::checklist_tray(id, chat, cx))
                                        .map(|tray| {
                                            div()
                                                .absolute()
                                                .left(px(TRAY_INSET))
                                                .right(px(TRAY_INSET))
                                                .bottom(relative(1.0))
                                                .child(tray)
                                        }),
                                )
                                .child(self.surface_composer(id, chat, cx)),
                        ),
                ),
            )
            .child(div().flex_none().h(relative(CENTRED_LIFT * (1.0 - landed))))
            .into_any_element()
    }

    fn agent_notice(
        &self,
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let now = self.agent_now(id);
        let terminal_backed = chat.structured().is_none();
        let waits_unseen = now == AgentNow::Waiting && chat.asks().is_empty();
        if !terminal_backed {
            return None;
        }
        let text = match now {
            AgentNow::Waiting if waits_unseen => {
                format!("{} {WAITING_TEXT}", chat.harness().display_name())
            }
            AgentNow::Exited => EXITED_TEXT.to_owned(),
            AgentNow::Waiting | AgentNow::Idle | AgentNow::Working => return None,
        };
        let pal = palette(cx);
        let open = ElementId::Name(format!("{OPEN_TERMINAL_PREFIX}-{}", id.as_u64()).into());
        Some(
            tray(pal)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_2))
                        .child(icon(IconName::Terminal, ICON, pal.text_muted))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(rgb(pal.text_default))
                                .child(text),
                        )
                        .child(
                            tray_button(open, true, pal)
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.switch_to_terminal(id, cx);
                                }))
                                .child(OPEN_TERMINAL_LABEL),
                        ),
                )
                .into_any_element(),
        )
    }

    fn jump_down(id: TerminalId, thread: ListState, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(DOWN_BUTTON_OFFSET))
            .flex()
            .justify_center()
            .child(
                div()
                    .id((DOWN_PREFIX, id.as_u64()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(ROUND_BUTTON))
                    .rounded_full()
                    .cursor_pointer()
                    .bg(rgb(pal.bg_surface))
                    .shadow(floating_shadow(pal))
                    .tooltip(|_, cx| tooltip_view(TOOLTIP_DOWN.into(), cx))
                    .on_click(cx.listener(move |_: &mut Self, _, _, cx| {
                        thread.scroll_to_end();
                        cx.notify();
                    }))
                    .child(icon(IconName::ChevronDown, ICON, pal.text_default)),
            )
            .into_any_element()
    }
}

pub(super) fn column() -> gpui_kit::Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .max_w(px(COLUMN_MAX_W))
        .px(px(COLUMN_PAD_X))
}

pub(super) fn muted_line(text: &'static str, pal: &Palette) -> AnyElement {
    div()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_muted))
        .child(text)
        .into_any_element()
}

pub(super) fn error_line(text: &str, pal: &Palette) -> AnyElement {
    div()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.error_fg))
        .child(text.to_owned())
        .into_any_element()
}
