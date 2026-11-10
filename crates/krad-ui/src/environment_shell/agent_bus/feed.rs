use krad_agents::{
    AgentInfo, AgentKey, ErrorCode, Request, Response, StateReason, TerminalState, Timestamp,
};
use krad_domain::{AgentState, AttentionKind, EnvironmentName, NodeKind, TerminalId, TerminalNode};

use super::super::live_env::LiveEnv;
use super::super::EnvironmentShell;

const STREAMED_BY_THE_BUS: &str =
    "The bus streams agent events on its own connection; this request never reaches the app";
const CLIENTS_ONLY_READ: &str =
    "Programs outside the app's terminals can list and watch agents; other requests need a Krad ADE terminal";

pub(super) fn streamed_by_the_bus() -> Response {
    Response::error(ErrorCode::Unsupported, STREAMED_BY_THE_BUS)
}

pub(super) const fn lists_as_agent(node: &TerminalNode) -> bool {
    node.file.is_none() && matches!(node.kind, NodeKind::Terminal | NodeKind::Chat)
}

fn client_refusal() -> Response {
    Response::error(ErrorCode::Forbidden, CLIENTS_ONLY_READ)
}

pub(super) fn client_answer(shell: &EnvironmentShell, request: &Request) -> Response {
    match request {
        Request::List => Response::Agents(shell.every_agent_info()),
        Request::ListAll => Response::Agents(
            shell
                .envs
                .keys()
                .flat_map(|name| shell.all_agent_infos(name.as_str(), None))
                .collect(),
        ),
        _ => client_refusal(),
    }
}

impl EnvironmentShell {
    pub(super) fn agent_info(
        &self,
        env: &EnvironmentName,
        live: &LiveEnv,
        node: &TerminalNode,
    ) -> AgentInfo {
        let session = live.sessions.get(&node.id);
        let chat = self.chat_presence(env, node.id).is_some();
        let alive = chat || session.is_some_and(|session| session.state.is_running());
        AgentInfo {
            environment: Some(env.as_str().to_owned()),
            terminal: node.id.as_u64(),
            title: node.title.clone(),
            group: live.data.group_of(node.id).map(|group| group.name.clone()),
            state: if alive {
                TerminalState::Running(node.state)
            } else {
                TerminalState::Exited
            },
            harness: session.and_then(|session| session.harness).or(node.harness),
            is_self: false,
        }
    }

    fn every_agent_info(&self) -> Vec<AgentInfo> {
        self.envs
            .iter()
            .flat_map(|(env, live)| {
                live.data
                    .nodes
                    .iter()
                    .filter(|node| lists_as_agent(node))
                    .map(move |node| self.agent_info(env, live, node))
            })
            .collect()
    }

    pub(in crate::environment_shell) fn publish_agent(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        hint: Option<StateReason>,
    ) {
        let Some(bus) = &self.agent_bus else {
            return;
        };
        let seen = self.envs.get(env).and_then(|live| {
            let node = live.data.nodes.iter().find(|node| node.id == id)?;
            lists_as_agent(node).then(|| self.agent_info(env, live, node))
        });
        let hint = hint.or_else(|| self.waiting_reason(env, id, seen.as_ref()));
        bus.endpoint().feed.observe(
            AgentKey::new(env.as_str(), id.as_u64()),
            seen,
            hint,
            Timestamp::now(),
        );
    }

    pub(in crate::environment_shell) fn reconcile_agent_feed(&self) {
        let Some(bus) = &self.agent_bus else {
            return;
        };
        let environments: Vec<String> = self
            .envs
            .keys()
            .map(|name| name.as_str().to_owned())
            .collect();
        bus.endpoint()
            .feed
            .reconcile(&environments, self.every_agent_info(), Timestamp::now());
    }

    fn waiting_reason(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        seen: Option<&AgentInfo>,
    ) -> Option<StateReason> {
        let waiting = TerminalState::Running(AgentState::WaitingForInput);
        if seen.is_none_or(|agent| agent.state != waiting) {
            return None;
        }
        match self.front_ask(env, id)?.kind {
            AttentionKind::Permission => Some(StateReason::PermissionPending),
            AttentionKind::Ask => Some(StateReason::AskPending),
            AttentionKind::Error | AttentionKind::Finished => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(kind: NodeKind) -> TerminalNode {
        TerminalNode {
            id: TerminalId(3),
            title: "Codex · 3".to_owned(),
            state: AgentState::Idle,
            harness: None,
            resume: None,
            group: None,
            text_scale: krad_domain::TextScale::default(),
            kind,
            file: None,
            extension: None,
            bookmarks: krad_domain::ChatBookmarks::default(),
            usage: krad_domain::ChatUsage::default(),
            checkpoints: krad_domain::ChatCheckpoints::default(),
            choice: krad_domain::ChatChoice::default(),
        }
    }

    #[test]
    fn terminals_and_chats_list_as_agents_but_files_and_extension_tabs_do_not() {
        assert!(lists_as_agent(&node(NodeKind::Terminal)));
        assert!(lists_as_agent(&node(NodeKind::Chat)));
        assert!(!lists_as_agent(&node(NodeKind::File)));
        assert!(!lists_as_agent(&node(NodeKind::Extension)));
    }

    #[test]
    fn requests_a_client_cannot_make_are_refused_with_stable_codes() {
        assert!(matches!(
            client_refusal(),
            Response::Error(error) if error.code == ErrorCode::Forbidden
        ));
        assert!(matches!(
            streamed_by_the_bus(),
            Response::Error(error) if error.code == ErrorCode::Unsupported
        ));
    }
}
