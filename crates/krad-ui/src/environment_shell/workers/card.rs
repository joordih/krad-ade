use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::{AgentState, EnvironmentName, TerminalId};

use super::super::chat::{AnswerPart, ChatMessage};
use super::super::EnvironmentShell;
use crate::components::{agent_status, icon, IconName, StatusKind};
use crate::theme::{
    palette, Palette, ICON_SM, LINE_SM, LINE_XS, RADIUS_MD, SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM,
    TEXT_XS,
};

const CARD_PREFIX: &str = "surface-worker";
const WORKER_LABEL: &str = "Worker";
const STARTING_LABEL: &str = "Starting…";
const CLOSED_LABEL: &str = "Finished · open to read or continue it";
const SPAWN_VERB: &str = " spawn ";
const ESCAPE: char = '\\';

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct WorkerCard {
    pub(in crate::environment_shell) terminal: TerminalId,
    pub(in crate::environment_shell) task: String,
    pub(in crate::environment_shell) state: Option<AgentState>,
    pub(in crate::environment_shell) activity: Option<String>,
}

pub(in crate::environment_shell) fn spawned_worker<'a>(
    command: &str,
    cards: &'a [WorkerCard],
) -> Option<&'a WorkerCard> {
    if cards.is_empty() || !command.contains(krad_agents::CLI_PROGRAM) {
        return None;
    }
    if !command.contains(SPAWN_VERB) {
        return None;
    }
    let command = unescaped(command);
    cards.iter().find(|card| {
        let task = unescaped(card.task.trim());
        !task.is_empty() && command.contains(task.as_str())
    })
}

fn unescaped(text: &str) -> String {
    text.chars().filter(|char| *char != ESCAPE).collect()
}

fn latest_activity(messages: &[ChatMessage]) -> Option<String> {
    messages
        .iter()
        .rev()
        .flat_map(|message| message.parts.iter().rev())
        .find_map(|part| match part {
            AnswerPart::Tool(call) => Some(format!("{} {}", call.name, call.summary)),
            AnswerPart::Text(text) => text
                .lines()
                .rev()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(str::to_owned),
            AnswerPart::Thinking(_) | AnswerPart::Ask(_) => None,
        })
}

fn status_mark(state: Option<AgentState>, pal: &Palette) -> AnyElement {
    match state {
        Some(state) => agent_status(StatusKind::Agent(state), ICON_SM, pal).into_any_element(),
        None => icon(IconName::Tick, ICON_SM, pal.text_muted).into_any_element(),
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn worker_cards(
        &self,
        lead_chat: TerminalId,
    ) -> Vec<WorkerCard> {
        let env = &self.active;
        let Some(live) = self.envs.get(env) else {
            return Vec::new();
        };
        let Some(lead) = live
            .data
            .session_led_by(lead_chat)
            .and_then(|session| live.data.lead(session))
        else {
            return Vec::new();
        };
        lead.workers
            .iter()
            .map(|worker| {
                let open = worker.session.is_some();
                let state = open
                    .then(|| {
                        live.data
                            .nodes
                            .iter()
                            .find(|node| node.id == worker.terminal)
                    })
                    .flatten()
                    .map(|node| node.state);
                WorkerCard {
                    terminal: worker.terminal,
                    task: worker.task.clone(),
                    state,
                    activity: self.worker_activity(env, worker.terminal, open),
                }
            })
            .collect()
    }

    fn worker_activity(
        &self,
        env: &EnvironmentName,
        terminal: TerminalId,
        open: bool,
    ) -> Option<String> {
        if !open {
            return Some(CLOSED_LABEL.to_owned());
        }
        if self.is_launching(env, terminal) {
            return Some(STARTING_LABEL.to_owned());
        }
        let chat = self.envs.get(env)?.chats.get(&terminal)?;
        latest_activity(&chat.messages)
    }

    pub(in crate::environment_shell) fn worker_card(
        owner: TerminalId,
        card: &WorkerCard,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let terminal = card.terminal;
        div()
            .id(ElementId::Name(SharedString::from(format!(
                "{CARD_PREFIX}-{}-{}",
                owner.as_u64(),
                terminal.as_u64()
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
                this.go_to_worker(terminal, cx);
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
                            .child(WORKER_LABEL),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(pal.text_muted))
                            .child(card.task.clone()),
                    )
                    .child(status_mark(card.state, pal))
                    .child(icon(IconName::ChevronRight, ICON_SM, pal.text_muted)),
            )
            .children(card.activity.clone().map(|activity| {
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

    fn go_to_worker(&mut self, terminal: TerminalId, cx: &mut Context<Self>) {
        let env = self.active.clone();
        if !self.reopen_worker(&env, terminal, cx) {
            return;
        }
        self.focus_terminal(terminal);
        cx.notify();
        self.store();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(terminal: u64, task: &str) -> WorkerCard {
        WorkerCard {
            terminal: TerminalId(terminal),
            task: task.to_owned(),
            state: None,
            activity: None,
        }
    }

    #[test]
    fn a_spawn_command_finds_the_worker_it_started_by_its_task() {
        let cards = [
            card(3, "Fix the login form"),
            card(4, "Write the \"retry\" tests"),
        ];
        let command = r#"krad-ade-agents spawn --agent claude "Write the \"retry\" tests""#;
        assert_eq!(spawned_worker(command, &cards), Some(&cards[1]));
        let listing = "krad-ade-agents list";
        assert_eq!(spawned_worker(listing, &cards), None);
        let other = "cargo spawn Fix the login form";
        assert_eq!(spawned_worker(other, &cards), None);
    }
}
