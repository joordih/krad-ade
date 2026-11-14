use gpui_kit::App;
use krad_domain::{EnvironmentName, NodeKind, Prompt, TerminalId};

use super::super::EnvironmentShell;
use super::{ChatPlace, ChatSeed};

impl EnvironmentShell {
    pub(in crate::environment_shell) fn runs_chat(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> bool {
        self.runtime.chats.contains(&ChatPlace::here(env).chat(id))
            || self
                .envs
                .get(env)
                .is_some_and(|live| live.chats.contains_key(&id))
    }

    pub(in crate::environment_shell) fn queue_chat_prompt(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        text: &str,
        cx: &mut App,
    ) -> bool {
        let is_chat = self.envs.get(env).is_some_and(|live| {
            live.data
                .nodes
                .iter()
                .any(|node| node.id == id && node.kind == NodeKind::Chat)
        });
        if !is_chat {
            return false;
        }
        let seed = ChatSeed {
            prompts: vec![Prompt::from(text)],
            ..ChatSeed::default()
        };
        self.open_chat(env, id, seed, cx);
        true
    }
}
