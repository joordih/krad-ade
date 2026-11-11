use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, relative, rgb, AnyElement, Context, ElementId, ExternalPaths, FontWeight,
    InteractiveElement, IntoElement, ListState, ParentElement, ScrollHandle, ScrollWheelEvent,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{AgentState, TerminalId};

use super::super::ask::{tray, tray_button};
use super::super::rubber_band::{band_watch, BandKey, ScrollEdges};
use super::super::sidebar::PanelReveal;
use super::super::EnvironmentShell;
use super::dot_field::dot_field;
use super::SurfaceChat;
use crate::components::{icon, tooltip_view, IconName};
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
    thread.is_following_tail() || thread.is_scrolled_to_end() == Some(true)
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
            let started = self.turn_started(id, chat);
            return Self::subagent_view(id, tool, chat, started, &self.copy_feedback, cx);
        }
        let scale = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
            .map_or(krad_domain::TextScale::DEFAULT, |node| {
                node.text_scale.as_f32()
            });
        if chat.text_scale.replace(scale).to_bits() != scale.to_bits() {
            chat.thread.remeasure();
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
            .children(self.chat_backdrop(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .opacity(landed)
                    .child(self.bouncing_thread(id, chat, now, cx))
                    .children(self.timeline_strip(id, chat, cx))
                    .when(!following, |area| area.child(Self::jump_down(id, cx))),
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
                                    self.ask_tray(id, cx)
                                        .or_else(|| self.acp_sign_in_tray(id, chat, cx))
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
            .child(div().flex_1().min_h_0().flex_grow(1.0 - landed))
            .into_any_element()
    }

    fn bouncing_thread(
        &self,
        id: TerminalId,
        chat: &SurfaceChat,
        now: AgentNow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bounce = chat.thread.clone();
        let edges = move || ScrollEdges {
            offset: -bounce.scroll_px_offset_for_scrollbar().y.as_f32(),
            max: bounce.max_offset_for_scrollbar().y.as_f32(),
        };
        div()
            .relative()
            .size_full()
            .top(px(self.gestures.bands.offset(BandKey::Chat(id))))
            .on_scroll_wheel(
                cx.listener(move |this: &mut Self, _: &ScrollWheelEvent, _, _| {
                    this.scrolled_by_hand(id);
                }),
            )
            .child(Self::surface_thread(id, chat, now, cx))
            .child(band_watch(BandKey::Chat(id), edges, cx))
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
                format!(
                    "{} {WAITING_TEXT}",
                    self.settings.acp.harness_name(chat.harness())
                )
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

    fn jump_down(id: TerminalId, cx: &mut Context<Self>) -> AnyElement {
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
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.jump_to_latest(id);
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{FollowMode, ListAlignment};

    fn thread() -> ListState {
        let thread = ListState::new(3, ListAlignment::Top, px(0.0));
        thread.set_follow_mode(FollowMode::Tail);
        thread
    }

    #[test]
    fn a_new_thread_follows_its_latest_message() {
        assert!(following(&thread()));
    }

    #[test]
    fn a_thread_scrolled_away_stays_put_while_its_messages_are_remeasured() {
        let thread = thread();
        thread.pause_following_tail();
        thread.remeasure();
        assert!(!following(&thread));
    }
}
