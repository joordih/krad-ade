use gpui_kit::Context;
use krad_chat::{new_session_id, LiveSession};
use krad_domain::SessionSpec;
use krad_domain::{
    AgentPreset, AgentState, EnvironmentName, GroupId, NodeKind, Route, TerminalId, TerminalNode,
    TextScale,
};
use krad_session::wsl_shell;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use super::sessions::Launch;
use super::sessions_view::session_terminals;
use super::EnvironmentShell;

const NEW_CHAT_LABEL: &str = "New chat";
const DEFAULT_CHAT_HARNESSES: [AgentPreset; 3] = [
    AgentPreset::ClaudeCode,
    AgentPreset::Opencode,
    AgentPreset::Codex,
];
const WSL_LABEL: &str = "WSL";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NodeGroup {
    OrSelected(Option<GroupId>),
    Exactly(Option<GroupId>),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct NodeSpec<'a> {
    pub(super) kind: NodeKind,
    pub(super) harness: Option<AgentPreset>,
    pub(super) label: &'a str,
    pub(super) group: NodeGroup,
    pub(super) route: Route,
}

impl<'a> NodeSpec<'a> {
    pub(super) const fn terminal(label: &'a str, group: Option<GroupId>, route: Route) -> Self {
        Self {
            kind: NodeKind::Terminal,
            harness: None,
            label,
            group: NodeGroup::OrSelected(group),
            route,
        }
    }
}

impl EnvironmentShell {
    pub(super) fn alloc_terminal_id(&mut self, env: &EnvironmentName) -> Option<TerminalId> {
        let live = self.envs.get_mut(env)?;
        let id = TerminalId(live.data.next_id);
        live.data.next_id += 1;
        Some(id)
    }

    pub(super) fn add_terminal(&mut self, preset: AgentPreset, cx: &mut Context<Self>) {
        if self.settings.opens_as_chat(preset) {
            let env = self.active.clone();
            let session = LiveSession::Start(new_session_id());
            self.spawn_chat_in(&env, NodeGroup::OrSelected(None), session, cx);
            return;
        }
        self.spawn_terminal(preset.display_name(), Launch::Preset(preset), None, cx);
    }

    pub(super) fn add_new_tab(&mut self, cx: &mut Context<Self>) {
        self.add_chat_tab(cx);
    }

    pub(super) fn start_chat_session(&mut self, group: Option<GroupId>, cx: &mut Context<Self>) {
        self.new_session(group, SessionSpec::default(), cx);
        let env = self.active.clone();
        let session = LiveSession::Start(new_session_id());
        self.spawn_chat_in(&env, NodeGroup::Exactly(group), session, cx);
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
        let route = self.new_terminal_route(&launch);
        let spec = NodeSpec::terminal(label, group, route);
        self.open_terminal(env, &spec, launch, after, cx)
    }

    fn new_terminal_route(&self, launch: &Launch) -> Route {
        launch
            .harness()
            .filter(|harness| self.settings.opens_as_chat(*harness))
            .map_or(Route::Direct, |harness| {
                self.settings.gateway.new_chat_route(harness)
            })
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

    pub(super) fn spawn_chat_in(
        &mut self,
        env: &EnvironmentName,
        group: NodeGroup,
        session: LiveSession,
        cx: &mut Context<Self>,
    ) -> Option<TerminalId> {
        let preset = self.default_chat_harness();
        let spec = NodeSpec {
            kind: NodeKind::Chat,
            harness: Some(preset),
            label: NEW_CHAT_LABEL,
            group,
            route: self.settings.gateway.new_chat_route(preset),
        };
        let (id, _) = self.add_node(env, &spec, None)?;
        if let Some(live) = self.envs.get_mut(env) {
            live.pending_chats.insert(id, session);
        }
        self.settle_new_node(cx);
        Some(id)
    }

    fn default_chat_harness(&self) -> AgentPreset {
        DEFAULT_CHAT_HARNESSES
            .into_iter()
            .find(|preset| self.native_agents.contains(preset))
            .unwrap_or(AgentPreset::ClaudeCode)
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
            route: spec.route,
        });
        live.data
            .place_terminal(id, anchor_node.as_ref().map(|node| node.id));
        live.focused = Some(id);
        Some((id, cwd))
    }

    pub(super) fn close_terminal(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        if let Some(session) = live.sessions.remove(&id) {
            session.cancelled.store(true, Ordering::SeqCst);
            if let Some(killer) = session.killer {
                killer.kill();
            }
        }
        live.chats.remove(&id);
        live.pending_chats.remove(&id);
        live.data.nodes.retain(|node| node.id != id);
        live.data.meta.remove(&id);
        live.mailbox.forget(id.as_u64());
        live.data.remove_terminal_tiles(id);
        if live.focused == Some(id) {
            live.focused = live.data.nodes.first().map(|node| node.id);
        }
        cx.notify();
        self.store();
    }
}
