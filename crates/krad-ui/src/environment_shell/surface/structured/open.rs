use gpui_kit::{App, Context, Window};
use krad_domain::{
    new_session_id, AcpWanted, AgentPreset, AgentState, ChatChoice, EnvironmentName, LiveSession,
    NodeKind, PermissionMode, Prompt, RuntimeId, TerminalId, DEFAULT_CHAT_HARNESS,
};
use krad_protocol::{ChatCommand, ChatMirror, ChatOpen, ChatState};

use super::super::super::session_templates::ChatChoices;
use super::super::super::EnvironmentShell;
use super::super::{Backend, ChatPlace, ChatView};
use super::hosted::{mirrored_structured, mirrored_summary};

#[derive(Debug, Default)]
pub(in crate::environment_shell) struct ChatSeed {
    pub(in crate::environment_shell) session: Option<LiveSession>,
    pub(in crate::environment_shell) prompts: Vec<Prompt>,
    pub(in crate::environment_shell) mode: Option<PermissionMode>,
    pub(in crate::environment_shell) choices: ChatChoices,
}

impl From<LiveSession> for ChatSeed {
    fn from(session: LiveSession) -> Self {
        Self {
            session: Some(session),
            ..Self::default()
        }
    }
}

fn opening_state(start: &ChatOpen, prompts: &[Prompt]) -> ChatState {
    let mut state = ChatState::new(start.harness);
    state.chosen_model.clone_from(&start.model);
    state.effort = start.effort;
    state.permission_mode = start.permission_mode;
    state.resume = start.session.history().cloned();
    state.queued = prompts.to_vec();
    state
}

impl EnvironmentShell {
    fn chat_harness(&self, env: &EnvironmentName, id: TerminalId) -> Option<AgentPreset> {
        let node = self
            .envs
            .get(env)?
            .data
            .nodes
            .iter()
            .find(|node| node.id == id && node.kind == NodeKind::Chat)?;
        Some(
            node.harness
                .filter(|preset| preset.has_structured_chat())
                .unwrap_or(DEFAULT_CHAT_HARNESS),
        )
    }

    fn node_session(&self, env: &EnvironmentName, id: TerminalId) -> LiveSession {
        self.envs
            .get(env)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
            .and_then(|node| node.resume.clone())
            .map_or_else(|| LiveSession::Start(new_session_id()), LiveSession::Resume)
    }

    pub(in crate::environment_shell) fn open_chat(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        seed: ChatSeed,
        cx: &mut App,
    ) {
        let place = ChatPlace::here(env);
        if self.runtime.chats.contains(&place.chat(id)) {
            if let Some(mode) = seed.mode {
                let chat = place.chat(id);
                self.post_chat(&place, ChatCommand::SetPermissionMode { chat, mode });
            }
            for prompt in seed.prompts {
                self.send_prompt(&place, id, prompt, cx);
            }
            return;
        }
        let Some(harness) = self.chat_harness(env, id) else {
            return;
        };
        let ChatSeed {
            session,
            prompts,
            mode,
            choices,
        } = seed;
        let session = session.unwrap_or_else(|| self.node_session(env, id));
        let explicit = ChatChoice {
            model: choices.model,
            effort: choices.effort,
        };
        let choice = self.restored_choice(env, id, harness, explicit, cx);
        self.keep_choice(env, id, choice.clone(), cx);
        let start = ChatOpen {
            harness,
            model: choice.model,
            effort: choice.effort,
            permission_mode: mode,
            mcp_servers: choices.mcp_servers,
            acp: AcpWanted::default(),
            session,
            messages: Vec::new(),
            notices: Vec::new(),
        };
        let mirror = ChatMirror::new(opening_state(&start, &prompts));
        let chat = place.chat(id);
        self.runtime
            .edit_chats(cx, |chats| chats.opening(chat.clone(), mirror));
        self.post_chat(
            &place,
            ChatCommand::Open {
                chat,
                start: Box::new(start),
            },
        );
        let starts_turn = !prompts.is_empty();
        for prompt in prompts {
            let chat = place.chat(id);
            self.post_chat(&place, ChatCommand::Send { chat, prompt });
        }
        if starts_turn {
            self.set_agent_state(env, id, AgentState::Working, cx);
        }
        tracing::debug!(terminal = id.as_u64(), "chat opened in the runtime");
    }

    pub(in crate::environment_shell) fn open_restored_chats(
        &mut self,
        env: &EnvironmentName,
        ids: impl IntoIterator<Item = TerminalId>,
        cx: &mut App,
    ) {
        for id in ids {
            self.open_chat(env, id, ChatSeed::default(), cx);
        }
    }

    pub(in crate::environment_shell) fn ensure_structured_chats(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let background: Vec<EnvironmentName> = self
            .envs
            .keys()
            .filter(|name| self.runs_in_background(name, cx))
            .cloned()
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
        let Some(live) = self.envs.get(env) else {
            return;
        };
        let place = ChatPlace::here(env);
        let unseen: Vec<TerminalId> = live
            .data
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Chat && !live.chats.contains_key(&node.id))
            .map(|node| node.id)
            .filter(|id| self.runtime.chats.contains(&place.chat(*id)))
            .collect();
        for id in unseen {
            self.attach_chat_view(&place, id, window, cx);
        }
    }

    fn attach_chat_view(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mirror) = self.runtime.chats.get(&place.chat(id)).cloned() else {
            return;
        };
        let (input, events) = Self::chat_input(id, window, cx);
        let structured = mirrored_structured(&mirror);
        let harness = structured.harness;
        let chat = ChatView::new(input, events, Backend::Structured(Box::new(structured)));
        let Some(live) = self.live_mut(place) else {
            return;
        };
        live.chats.insert(id, chat);
        let env = place.environment.clone();
        self.watch_subagents(&env, id, cx);
        if let AgentPreset::Acp(agent) = harness {
            self.seed_acp_chats(cx);
            self.list_acp_models(agent, cx);
        }
        let summary = mirrored_summary(place, id, mirror);
        self.runtime_chats_replaced(RuntimeId::Local, vec![summary], cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{Effort, ModelId, ResumeId};

    #[test]
    fn an_opening_chat_reads_as_its_choices_with_its_first_prompts_queued() {
        let resume: ResumeId = "4f2c0b7c-a91e"
            .parse()
            .unwrap_or_else(|_| unreachable!("a valid resume id"));
        let start = ChatOpen {
            harness: AgentPreset::ClaudeCode,
            model: ModelId::parse("opus"),
            effort: Some(Effort::High),
            permission_mode: Some(PermissionMode::Plan),
            mcp_servers: None,
            acp: AcpWanted::default(),
            session: LiveSession::Resume(resume.clone()),
            messages: Vec::new(),
            notices: Vec::new(),
        };
        let state = opening_state(&start, &[Prompt::from("Fix the race")]);
        assert_eq!(state.harness, AgentPreset::ClaudeCode);
        assert_eq!(state.chosen_model, ModelId::parse("opus"));
        assert_eq!(state.effort, Some(Effort::High));
        assert_eq!(state.permission_mode, Some(PermissionMode::Plan));
        assert_eq!(state.resume, Some(resume));
        assert_eq!(state.queued, [Prompt::from("Fix the race")]);
        assert!(!state.working);
    }
}
