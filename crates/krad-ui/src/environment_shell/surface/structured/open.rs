use gpui_kit::{Context, Window};
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    AgentPreset, ChatChoice, EnvironmentName, NodeKind, TerminalId,
    TerminalNode, DEFAULT_CHAT_HARNESS,
};
use std::collections::HashMap;

use super::super::super::EnvironmentShell;
use super::super::{Backend, SurfaceChat};
use super::{structured_chat, StructuredChat};


pub(in crate::environment_shell) fn restored_chats(
    nodes: &[TerminalNode],
) -> HashMap<TerminalId, LiveSession> {
    nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Chat)
        .map(|node| {
            let session = node
                .resume
                .clone()
                .map_or_else(|| LiveSession::Start(new_session_id()), LiveSession::Resume);
            (node.id, session)
        })
        .collect()
}

const CONNECTING_TEXT: &str = "Connecting to";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn ensure_structured_chats(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let background: Vec<EnvironmentName> = self
            .envs
            .iter()
            .filter(|(_, live)| !live.pending_chats.is_empty())
            .map(|(name, _)| name.clone())
            .filter(|name| self.runs_in_background(name))
            .collect();
        let active = self.active.clone();
        for env in std::iter::once(active).chain(background) {
            self.ensure_structured_chats_of(&env, window, cx);
        }
    }

    fn ensure_structured_chats_of(
        &mut self,
        env: &EnvironmentName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        if live.pending_chats.is_empty() {
            return;
        }
        let nodes = &live.data.nodes;
        let pending: Vec<(TerminalId, LiveSession, AgentPreset)> = live
            .pending_chats
            .drain()
            .filter_map(|(id, session)| {
                let node = nodes.iter().find(|node| node.id == id)?;
                let harness = node
                    .harness
                    .filter(|preset| preset.has_structured_chat())
                    .unwrap_or(DEFAULT_CHAT_HARNESS);
                Some((id, session, harness))
            })
            .collect();
        for (id, session, harness) in pending {
            let (input, events) = Self::chat_input(id, window, cx);
            let _history = session.history().cloned();
            let mut structured = StructuredChat::new(session, harness);
            let live = self.envs.get_mut(env);
            let (waiting, mode, choices) = live.map_or_else(Default::default, |live| {
                (
                    live.waiting_prompts.remove(&id).unwrap_or_default(),
                    live.waiting_modes.remove(&id),
                    live.waiting_choices.remove(&id).unwrap_or_default(),
                )
            });
            let choice = self.restored_choice(
                env,
                id,
                harness,
                ChatChoice {
                    model: choices.model,
                    effort: choices.effort,
                },
            );
            self.keep_choice(env, id, choice.clone());
            structured.permission_mode = mode;
            structured.chosen_model = choice.model;
            structured.mcp_servers = choices.mcp_servers;
            structured.effort = choice.effort;
            let mut chat =
                SurfaceChat::new(input, events, Backend::Structured(Box::new(structured)));
            let prompted = !waiting.is_empty();
            for prompt in waiting {
                chat.outbox.push(prompt);
            }
            if let Some(live) = self.envs.get_mut(env) {
                live.chats.insert(id, chat);
            }
            self.watch_subagents(env, id, cx);
            if let AgentPreset::Acp(agent) = harness {
                self.seed_acp_chats();
                self.list_acp_models(agent, cx);
            }
            self.runtime_chat_opened(env, id, cx);
            if prompted {
                self.flush_structured(env, id, cx);
            }
        }
    }

    pub(super) fn note_history_waits_for_host(&mut self, env: &EnvironmentName, id: TerminalId) {
        let Some(destination) = self.envs.get(env).map(|live| live.data.host.to_string()) else {
            return;
        };
        if let Some(chat) = structured_chat(self, env, id) {
            chat.problem = Some(format!("{CONNECTING_TEXT} {destination}"));
        }
    }

    pub(in crate::environment_shell) fn load_missing_histories(
        &mut self,
        env: &EnvironmentName,
        cx: &mut Context<Self>,
    ) {
        let opening: Vec<TerminalId> = self
            .envs
            .get(env)
            .map_or_else(Vec::new, |live| live.chats.keys().copied().collect());
        for id in opening {
            self.runtime_chat_opened(env, id, cx);
        }
    }

}
