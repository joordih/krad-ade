use gpui_kit::Context;
use krad_domain::{EnvironmentName, Prompt, TerminalId};

use super::super::chat::conversation_lines;
use super::super::EnvironmentShell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum ChatPresence {
    Queued,
    Open,
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn chat_presence(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<ChatPresence> {
        let live = self.envs.get(env)?;
        if live.waiting_prompts.contains_key(&id) {
            return Some(ChatPresence::Queued);
        }
        if let Some(chat) = live.chats.get(&id) {
            return Some(if chat.outbox.has_queued() {
                ChatPresence::Queued
            } else {
                ChatPresence::Open
            });
        }
        live.pending_chats
            .contains_key(&id)
            .then_some(ChatPresence::Open)
    }

    pub(in crate::environment_shell) fn chat_tail(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        lines: usize,
    ) -> Option<String> {
        let live = self.envs.get(env)?;
        let Some(chat) = live.chats.get(&id) else {
            return live.pending_chats.contains_key(&id).then(String::new);
        };
        let all = conversation_lines(&chat.messages);
        let start = all.len().saturating_sub(lines);
        Some(all[start..].join("\n"))
    }

    pub(in crate::environment_shell) fn queue_chat_prompt(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(live) = self.envs.get_mut(env) else {
            return false;
        };
        if let Some(chat) = live.chats.get_mut(&id) {
            chat.outbox.push(Prompt::from(text));
            self.flush_outbox(env, id, cx);
            return true;
        }
        if !live.pending_chats.contains_key(&id) {
            return false;
        }
        live.waiting_prompts
            .entry(id)
            .or_default()
            .push(Prompt::from(text));
        true
    }
}
