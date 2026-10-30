use gpui_kit::{AppContext as _, Context};
use krad_chat::{
    LiveSession, RewindError, RewindRequest, RewindScope, Rewound, DROPS_TURN_REFUSED,
};
use krad_domain::{AgentPreset, AgentState, EnvironmentName, Prompt, TerminalId};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::super::super::chat::{workplace_home, ChatMessage};
use super::super::super::EnvironmentShell;
use super::super::{Backend, SurfaceChat};
use super::{structured_chat, structured_mut, Batch, StructuredChat};

const STARTING_TEXT: &str = "The chat is still starting; send the edit again in a moment";

pub(in crate::environment_shell::surface) struct Rewinding {
    index: usize,
    prompt: Prompt,
}

fn cut_at(
    index: usize,
    messages: &mut Vec<ChatMessage>,
    opened: &mut HashSet<(usize, usize)>,
    groups: &mut HashMap<(usize, usize), bool>,
) {
    messages.truncate(index);
    opened.retain(|(message, _)| *message < index);
    groups.retain(|(message, _), _| *message < index);
}

fn cut_thread(chat: &mut SurfaceChat, index: usize) {
    cut_at(
        index,
        &mut chat.messages,
        &mut chat.opened_thoughts,
        &mut chat.tool_groups,
    );
    chat.thread.remeasure();
}

impl StructuredChat {
    pub(in crate::environment_shell::surface) const fn is_rewinding(&self) -> bool {
        self.rewinding.is_some()
    }

    pub(super) fn drops_turn_again(&mut self, error: &str) -> Option<Batch> {
        let confirmed = self.live.as_ref().is_some_and(|live| live.confirmed);
        let LiveSession::Cut(cut) = &mut self.session else {
            return None;
        };
        if confirmed || !error.contains(DROPS_TURN_REFUSED) {
            return None;
        }
        cut.drops.take()?;
        self.live = None;
        self.outstanding = 0;
        Some(self.last_batch.take().unwrap_or_default())
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn start_rewind(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        index: usize,
        prompt: Prompt,
        scope: RewindScope,
        cx: &mut Context<Self>,
    ) {
        let point = structured_chat(self, env, id)
            .and_then(|chat| chat.messages.get(index))
            .and_then(|message| message.rewind.clone());
        let Some(point) = point else {
            return;
        };
        if !self.start_or_report(env, id, cx) {
            if let Some(chat) = structured_chat(self, env, id) {
                chat.problem.get_or_insert_with(|| STARTING_TEXT.to_owned());
            }
            return;
        }
        let Some(structured) = structured_mut(self, env, id) else {
            return;
        };
        structured.working = true;
        structured.rewinding = Some(Rewinding { index, prompt });
        let harness = structured.harness;
        self.set_agent_state(env, id, AgentState::Working);
        let request = RewindRequest {
            point,
            scope,
            home: None,
        };
        if harness != AgentPreset::ClaudeCode {
            return self.hand_rewind(env, id, request);
        }
        let root = self
            .envs
            .get(env)
            .and_then(|live| live.data.group_of(id))
            .map(|group| group.root.clone());
        let found = cx.background_spawn(async move { workplace_home(root.as_deref()) });
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let home = found.await.ok().flatten();
            this.update(cx, |shell, cx| {
                shell.hand_rewind(&env, id, RewindRequest { home, ..request });
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn hand_rewind(&mut self, env: &EnvironmentName, id: TerminalId, request: RewindRequest) {
        match structured_mut(self, env, id).and_then(|structured| structured.live()) {
            Some(live) => live.rewind(request),
            None => self.rewound(env, id, Err(RewindError::Stopped)),
        }
    }

    pub(super) fn rewound(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        outcome: Result<Rewound, RewindError>,
    ) {
        let Some(chat) = structured_chat(self, env, id) else {
            return;
        };
        let Backend::Structured(structured) = &mut chat.backend else {
            return;
        };
        let Some(rewinding) = structured.rewinding.take() else {
            return;
        };
        structured.working = false;
        match outcome {
            Err(error) => {
                chat.problem = Some(error.to_string());
                self.set_agent_state(env, id, AgentState::Idle);
                return;
            }
            Ok(Rewound::Restart(session)) => {
                structured.session = session;
                structured.live = None;
            }
            Ok(Rewound::InPlace) => {}
        }
        cut_thread(chat, rewinding.index);
        chat.editing = None;
        chat.problem = None;
        chat.outbox.push_first(rewinding.prompt);
        chat.pins_latest = true;
        chat.start_glide(Instant::now());
        self.set_agent_state(env, id, AgentState::Idle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_chat::Speaker;

    fn message(speaker: Speaker, text: &str) -> ChatMessage {
        ChatMessage {
            speaker,
            text: text.to_owned(),
            tools: Vec::new(),
            parts: Vec::new(),
            preset: AgentPreset::Gateway,
            error: None,
            running: false,
            mentions: Vec::new(),
            checklist: None,
            rewind: None,
        }
    }

    #[test]
    fn a_rewind_drops_the_edited_question_and_everything_after_it() {
        let mut messages = vec![
            message(Speaker::User, "one"),
            message(Speaker::Assistant, "1"),
            message(Speaker::User, "two"),
            message(Speaker::Assistant, "2"),
            message(Speaker::User, "three"),
            message(Speaker::Assistant, "3"),
        ];
        let mut opened = HashSet::from([(1, 0), (3, 0)]);
        let mut groups = HashMap::from([((1, 2), true), ((5, 0), false)]);
        cut_at(2, &mut messages, &mut opened, &mut groups);
        let left: Vec<&str> = messages
            .iter()
            .map(|message| message.text.as_str())
            .collect();
        assert_eq!(left, ["one", "1"]);
        assert_eq!(opened, HashSet::from([(1, 0)]));
        assert_eq!(groups, HashMap::from([((1, 2), true)]));
    }
}
