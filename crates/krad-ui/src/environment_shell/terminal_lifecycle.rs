use gpui_kit::Context;
use krad_agents::StateReason;
use krad_chat::{new_session_id, LiveSession};
use krad_domain::SessionSpec;
use krad_domain::{
    AgentPreset, AgentState, CloudPlacement, EnvironmentName, GroupId, NodeKind, PermissionMode,
    Prompt, TerminalId, TerminalNode, TextScale, WorkerSpawn, DEFAULT_CHAT_HARNESS,
};
use krad_session::wsl_shell;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use super::sessions::Launch;
use super::sessions_view::{session_terminals, show_current_session};
use super::EnvironmentShell;

const NEW_CHAT_LABEL: &str = "New chat";
const WSL_LABEL: &str = "WSL";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NodeGroup {
    OrSelected(Option<GroupId>),
    Exactly(Option<GroupId>),
}

pub(super) struct WorkerChat {
    pub(super) group: Option<GroupId>,
    pub(super) preset: AgentPreset,
    pub(super) spawn: WorkerSpawn,
    pub(super) session: LiveSession,
    pub(super) prompts: Vec<Prompt>,
    pub(super) mode: Option<PermissionMode>,
    pub(super) cloud: Option<CloudPlacement>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct NodeSpec<'a> {
    pub(super) kind: NodeKind,
    pub(super) harness: Option<AgentPreset>,
    pub(super) label: &'a str,
    pub(super) group: NodeGroup,
}

impl EnvironmentShell {
    pub(super) fn alloc_terminal_id(&mut self, env: &EnvironmentName) -> Option<TerminalId> {
        let live = self.envs.get_mut(env)?;
        let id = TerminalId(live.data.next_id);
        live.data.next_id += 1;
        Some(id)
    }

    pub(super) fn add_terminal(&mut self, preset: AgentPreset, cx: &mut Context<Self>) {
        let label = self.settings.acp.harness_name(preset);
        self.spawn_terminal(&label, Launch::Preset(preset), None, cx);
    }

    pub(super) fn add_new_tab(&mut self, cx: &mut Context<Self>) {
        self.add_chat_tab(cx);
    }

    pub(super) fn start_chat_session(
        &mut self,
        group: Option<GroupId>,
        cx: &mut Context<Self>,
    ) -> Option<TerminalId> {
        self.new_session(group, SessionSpec::default(), cx);
        let env = self.active.clone();
        let session = LiveSession::Start(new_session_id());
        self.spawn_chat_in(&env, NodeGroup::Exactly(group), session, cx)
    }

    pub(super) fn ensure_chat_sessions(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.envs.get(&self.active) else {
            return;
        };
        let bare: Vec<GroupId> = live
            .data
            .groups
            .iter()
            .map(|group| group.id)
            .filter(|group| live.data.sessions_of(Some(*group)).next().is_none())
            .collect();
        let empty = live
            .data
            .current_session()
            .filter(|session| session_terminals(session).next().is_none())
            .and_then(|session| session.group);
        let shown = live.data.active_session;
        for group in bare {
            self.start_chat_session(Some(group), cx);
        }
        if let Some((shown, live)) = shown.zip(self.envs.get_mut(&self.active.clone())) {
            live.data.select_session(shown);
        }
        if let Some(group) = empty {
            let env = self.active.clone();
            let session = LiveSession::Start(new_session_id());
            self.spawn_chat_in(&env, NodeGroup::Exactly(Some(group)), session, cx);
        }
    }

    pub(super) fn add_chat_tab(&mut self, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let session = LiveSession::Start(new_session_id());
        self.spawn_chat_in(&env, NodeGroup::OrSelected(None), session, cx);
    }

    pub(super) fn add_wsl_terminal(&mut self, cx: &mut Context<Self>) {
        let (program, args) = wsl_shell(None);
        let vars = Vec::new();
        let launch = Launch::Command {
            program,
            args,
            vars,
        };
        self.spawn_terminal(WSL_LABEL, launch, None, cx);
    }

    pub(super) fn spawn_terminal(
        &mut self,
        label: &str,
        launch: Launch,
        after: Option<TerminalId>,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        self.spawn_terminal_in(&env, None, label, launch, after, cx);
    }

    pub(super) fn spawn_terminal_in(
        &mut self,
        env: &EnvironmentName,
        group: Option<GroupId>,
        label: &str,
        launch: Launch,
        after: Option<TerminalId>,
        cx: &mut Context<Self>,
    ) -> Option<TerminalId> {
        let spec = NodeSpec {
            kind: NodeKind::Terminal,
            harness: None,
            label,
            group: NodeGroup::OrSelected(group),
        };
        self.open_terminal(env, &spec, launch, after, cx)
    }

    pub(super) fn open_terminal(
        &mut self,
        env: &EnvironmentName,
        spec: &NodeSpec<'_>,
        launch: Launch,
        after: Option<TerminalId>,
        cx: &mut Context<Self>,
    ) -> Option<TerminalId> {
        let (id, cwd) = self.add_node(env, spec, after)?;
        self.start_session(env, id, launch, cwd, &[]);
        self.settle_new_node(cx);
        Some(id)
    }

    pub(super) fn open_terminal_at(
        &mut self,
        env: &EnvironmentName,
        spec: &NodeSpec<'_>,
        launch: Launch,
        cwd: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Option<TerminalId> {
        let (id, default_cwd) = self.add_node(env, spec, None)?;
        self.start_session(env, id, launch, cwd.or(default_cwd), &[]);
        self.settle_new_node(cx);
        Some(id)
    }

    pub(super) fn spawn_chat_in(
        &mut self,
        env: &EnvironmentName,
        group: NodeGroup,
        session: LiveSession,
        cx: &mut Context<Self>,
    ) -> Option<TerminalId> {
        let preset = self.default_chat_harness(env);
        self.spawn_chat_as(env, group, preset, session, cx)
    }

    pub(super) fn spawn_chat_as(
        &mut self,
        env: &EnvironmentName,
        group: NodeGroup,
        preset: AgentPreset,
        session: LiveSession,
        cx: &mut Context<Self>,
    ) -> Option<TerminalId> {
        let spec = NodeSpec {
            kind: NodeKind::Chat,
            harness: Some(preset),
            label: NEW_CHAT_LABEL,
            group,
        };
        let (id, _) = self.add_node(env, &spec, None)?;
        if let Some(live) = self.envs.get_mut(env) {
            live.pending_chats.insert(id, session);
        }
        self.draw_arcade_game();
        self.settle_new_node(cx);
        Some(id)
    }

    pub(super) fn spawn_seeded_chat(
        &mut self,
        env: &EnvironmentName,
        group: Option<GroupId>,
        harness: AgentPreset,
        session: LiveSession,
        prompts: Vec<Prompt>,
        cx: &mut Context<Self>,
    ) -> Option<TerminalId> {
        let label = self.settings.acp.harness_name(harness);
        let spec = NodeSpec {
            kind: NodeKind::Chat,
            harness: Some(harness),
            label: &label,
            group: NodeGroup::Exactly(group),
        };
        let (id, _) = self.add_node(env, &spec, None)?;
        let live = self.envs.get_mut(env)?;
        live.pending_chats.insert(id, session);
        if !prompts.is_empty() {
            live.waiting_prompts.insert(id, prompts);
        }
        self.settle_new_node(cx);
        Some(id)
    }

    pub(super) fn open_worker_chat(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        chat: WorkerChat,
        cx: &mut Context<Self>,
    ) -> Option<TerminalId> {
        let WorkerChat {
            group,
            preset,
            spawn,
            session,
            prompts,
            mode,
            cloud,
        } = chat;
        let spec = NodeSpec {
            kind: NodeKind::Chat,
            harness: Some(preset),
            label: &self.settings.acp.harness_name(preset),
            group: NodeGroup::OrSelected(group),
        };
        self.push_node(env, id, &spec, None)?;
        let live = self.envs.get_mut(env)?;
        let opened = live.data.open_worker_session(id, spawn);
        if let Some(opened) = opened.and_then(|opened| live.data.session_mut(opened)) {
            opened.cloud = cloud;
        }
        live.pending_chats.insert(id, session);
        if !prompts.is_empty() {
            live.waiting_prompts.insert(id, prompts);
        }
        if let Some(mode) = mode {
            live.waiting_modes.insert(id, mode);
        }
        self.settle_new_node(cx);
        Some(id)
    }

    fn default_chat_harness(&self, env: &EnvironmentName) -> AgentPreset {
        let available = self.agents_for(env, None);
        let chosen = self.settings.default_harness.as_offered(available);
        let preferred = chosen.has_structured_chat().then_some(chosen);
        preferred
            .into_iter()
            .chain(AgentPreset::chat_harnesses(&self.settings.acp.all_agents()))
            .find(|preset| self.chats_on(env, None, *preset) && preset.is_offered(available))
            .unwrap_or(DEFAULT_CHAT_HARNESS)
    }

    fn settle_new_node(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        self.store();
    }

    fn add_node(
        &mut self,
        env: &EnvironmentName,
        spec: &NodeSpec<'_>,
        after: Option<TerminalId>,
    ) -> Option<(TerminalId, Option<PathBuf>)> {
        let id = self.alloc_terminal_id(env)?;
        let (cwd, anchor) = self.push_node(env, id, spec, after)?;
        let live = self.envs.get_mut(env)?;
        live.data.place_terminal(id, anchor);
        live.focused = Some(id);
        Some((id, cwd))
    }

    fn push_node(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        spec: &NodeSpec<'_>,
        after: Option<TerminalId>,
    ) -> Option<(Option<PathBuf>, Option<TerminalId>)> {
        let title = format!("{} · {}", spec.label, id.as_u64());
        let live = self.envs.get_mut(env)?;
        let anchor_node = after.and_then(|anchor| {
            live.data
                .nodes
                .iter()
                .find(|node| node.id == anchor)
                .cloned()
        });
        let wanted = match spec.group {
            NodeGroup::OrSelected(group) => group.or(live.selected_group),
            NodeGroup::Exactly(group) => group,
        };
        let group = anchor_node
            .as_ref()
            .map_or(wanted, |node| node.group)
            .and_then(|group| live.data.group(group));
        let cwd = group.map(|group| group.root.clone());
        live.data.nodes.push(TerminalNode {
            id,
            title,
            state: AgentState::Idle,
            harness: spec.harness,
            resume: None,
            group: group.map(|group| group.id),
            text_scale: anchor_node
                .as_ref()
                .map_or_else(TextScale::default, |node| node.text_scale),
            kind: spec.kind,
            file: None,
            extension: None,
            bookmarks: krad_domain::ChatBookmarks::default(),
            usage: krad_domain::ChatUsage::default(),
            checkpoints: krad_domain::ChatCheckpoints::default(),
            choice: krad_domain::ChatChoice::default(),
            cloud: None,
        });
        Some((cwd, anchor_node.map(|node| node.id)))
    }

    pub(super) fn close_chat_tab(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        self.archive_chat_tab(env, id);
        self.close_terminal_in(env, id, cx);
    }

    pub(super) fn close_terminal(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        let env = self.active.clone();
        self.close_terminal_in(&env, id, cx);
    }

    pub(super) fn close_terminal_in(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        self.stop_cloud_node(env, id, cx);
        self.digest_ended_chat(env, id, cx);
        self.end_worker_launch(env, id);
        let workers = self
            .envs
            .get(env)
            .map_or_else(Vec::new, |live| live.data.workers_closing_with(id));
        for worker in workers {
            self.close_terminal_in(env, worker, cx);
        }
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        live.data.retire_chat_usage(id);
        if let Some(session) = live.sessions.remove(&id) {
            session.cancelled.store(true, Ordering::SeqCst);
            if let Some(killer) = session.killer {
                killer.kill();
            }
        }
        let chat = live.chats.remove(&id);
        live.pending_chats.remove(&id);
        live.waiting_prompts.remove(&id);
        live.waiting_modes.remove(&id);
        live.waiting_choices.remove(&id);
        live.data.drop_terminal(id);
        live.mailbox.forget(id.as_u64());
        if live.focused == Some(id) {
            live.focused = None;
            show_current_session(live);
            if live.focused.is_none() {
                live.focused = live.data.nodes.first().map(|node| node.id);
            }
        }
        self.forget_attention(env, id);
        self.publish_agent(env, id, Some(StateReason::UserClosed));
        let archived = self.archive_holds(env, id);
        if !archived && chat.is_some_and(|chat| chat.structured().is_some()) {
            Self::forget_chat_images(env, id, cx);
            self.forget_acp_transcript(env, id);
        }
        cx.notify();
        self.store();
    }
}
