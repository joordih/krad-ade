use gpui_kit::Context;
use krad_domain::{Prompt, TerminalId};
use krad_protocol::{Answer, ChatCommand, ChatPage, ChatState, Command, McpTrustAsk};
use std::time::{Instant, SystemTime};

use super::super::super::super::EnvironmentShell;
use super::super::super::outbox::{Outgoing, Stage};
use super::super::super::ChatPlace;
use super::super::{structured_chat, structured_mut, StructuredChat};
use super::structured_of;

const NOTICE_SEPARATOR: &str = "\n";

#[derive(Debug, Default)]
pub(in crate::environment_shell::surface) struct RuntimeShown {
    pub(in crate::environment_shell::surface) queued: Vec<Outgoing>,
    pub(in crate::environment_shell::surface) mcp_trust: Option<McpTrustAsk>,
    from: usize,
    earlier: bool,
    paging: bool,
    working_since: Option<(SystemTime, Instant)>,
}

impl RuntimeShown {
    pub(super) fn take_state(&mut self, state: &ChatState) {
        self.queued = state
            .queued
            .iter()
            .cloned()
            .map(|prompt: Prompt| Outgoing {
                prompt,
                stage: Stage::Queued,
            })
            .collect();
        self.earlier = state.earlier;
        self.mcp_trust.clone_from(&state.mcp_trust);
        self.working_since = match (state.working_since, self.working_since) {
            (Some(since), Some(known)) if known.0 == since => Some(known),
            (Some(since), _) => Some((since, as_instant(since, Instant::now(), SystemTime::now()))),
            (None, _) => None,
        };
    }

    pub(super) const fn shifted(&mut self, count: usize) {
        self.from += count;
    }

    pub(super) const fn local(&self, index: usize) -> Option<usize> {
        index.checked_sub(self.from)
    }

    pub(super) const fn starts_at(&mut self, from: usize) {
        self.from = from;
    }

    const fn skips_history(&self) -> bool {
        self.earlier || self.from > 0
    }
}

pub(super) fn shown_problem(state: &ChatState) -> Option<String> {
    let lines: Vec<&str> = state
        .problem
        .iter()
        .chain(&state.notices)
        .map(String::as_str)
        .collect();
    (!lines.is_empty()).then(|| lines.join(NOTICE_SEPARATOR))
}

fn as_instant(since: SystemTime, now: Instant, wall: SystemTime) -> Instant {
    let elapsed = wall.duration_since(since).unwrap_or_default();
    now.checked_sub(elapsed).unwrap_or(now)
}

impl StructuredChat {
    pub(in crate::environment_shell::surface) fn runtime_queued(&self) -> &[Outgoing] {
        if self.in_runtime() {
            &self.shown.queued
        } else {
            &[]
        }
    }

    pub(super) const fn runtime_from(&self) -> usize {
        self.shown.from
    }

    pub(in crate::environment_shell::surface) const fn runtime_skips_history(&self) -> bool {
        self.in_runtime() && self.shown.skips_history()
    }

    pub(in crate::environment_shell::surface) fn runtime_working_since(&self) -> Option<Instant> {
        self.shown
            .working_since
            .filter(|_| self.in_runtime())
            .map(|(_, since)| since)
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn unqueue_in_runtime(
        &self,
        place: &ChatPlace,
        id: TerminalId,
        index: usize,
    ) {
        self.post_runtime_chat(place, id, |chat| ChatCommand::Unqueue { chat, index });
    }

    pub(in crate::environment_shell::surface) fn show_earlier_in_runtime(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(shown) = structured_of(self, place, id)
            .filter(|structured| structured.in_runtime())
            .map(|structured| (structured.shown.from, structured.shown.earlier))
        else {
            return false;
        };
        match shown {
            (0, true) => {
                self.post_chat(
                    place,
                    ChatCommand::ShowEarlier {
                        chat: place.chat(id),
                    },
                );
            }
            (0, false) => {}
            _ => self.page_in_earlier(place, id, cx),
        }
        true
    }

    pub(super) fn page_in_earlier(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let Some(structured) = structured_mut(self, place, id).filter(|chat| chat.in_runtime())
        else {
            return;
        };
        let before = structured.shown.from;
        if structured.shown.paging || before == 0 {
            return;
        }
        structured.shown.paging = true;
        let command = Command::Chat(ChatCommand::Page {
            chat: place.chat(id),
            before,
            count: before,
        });
        let page = self.ask_at(place, command, cx);
        let place = place.clone();
        cx.spawn(async move |this, cx| {
            let page = page.await;
            this.update(cx, |shell, cx| {
                match page {
                    Ok(Answer::ChatPage(page)) => shell.earlier_page_read(&place, id, page),
                    Ok(_) | Err(_) => {
                        if let Some(structured) = structured_mut(shell, &place, id) {
                            structured.shown.paging = false;
                        }
                    }
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn earlier_page_read(&mut self, place: &ChatPlace, id: TerminalId, page: ChatPage) {
        let Some(chat) = structured_chat(self, place, id) else {
            return;
        };
        let Some(structured) = chat.chat.structured_mut() else {
            return;
        };
        structured.shown.paging = false;
        let Some(count) = structured.shown.from.checked_sub(page.from) else {
            return;
        };
        if page.messages.len() != count {
            return;
        }
        structured.shown.from = page.from;
        let mut messages = page.messages;
        messages.append(&mut chat.chat.messages);
        chat.chat.messages = messages;
        chat.opened_thoughts.clear();
        chat.tool_groups.clear();
        chat.editing = None;
        chat.thread.remeasure();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::AgentPreset;
    use std::time::Duration;

    #[test]
    fn notices_stay_under_the_runtime_problem() {
        let mut state = ChatState::new(AgentPreset::ClaudeCode);
        assert_eq!(shown_problem(&state), None);
        state.notices = vec!["The chosen model is not offered".to_owned()];
        assert_eq!(
            shown_problem(&state).as_deref(),
            Some("The chosen model is not offered")
        );
        state.problem = Some("The chat stopped".to_owned());
        assert_eq!(
            shown_problem(&state).as_deref(),
            Some("The chat stopped\nThe chosen model is not offered")
        );
    }

    #[test]
    fn the_runtime_queue_shows_as_queued_prompts_and_its_start_time_as_an_instant() {
        let mut shown = RuntimeShown::default();
        let mut state = ChatState::new(AgentPreset::Codex);
        state.queued = vec![Prompt::from("second")];
        state.earlier = true;
        let wall = SystemTime::now();
        state.working_since = wall.checked_sub(Duration::from_secs(90));
        shown.take_state(&state);
        assert_eq!(shown.queued.len(), 1);
        assert_eq!(shown.queued[0].stage, Stage::Queued);
        assert!(shown.skips_history());
        let now = Instant::now();
        let since = state
            .working_since
            .map(|since| as_instant(since, now, wall));
        assert_eq!(
            since.map(|since| now.duration_since(since)),
            Some(Duration::from_secs(90))
        );
        let first = shown.working_since;
        shown.take_state(&state);
        assert_eq!(shown.working_since, first, "the start does not drift");
        state.working_since = None;
        shown.take_state(&state);
        assert_eq!(shown.working_since, None);
    }
}
