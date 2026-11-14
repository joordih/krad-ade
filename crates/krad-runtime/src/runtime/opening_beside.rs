use krad_domain::{
    AgentState, ChatBookmarks, ChatCheckpoints, ChatChoice, ChatUsage, EnvironmentData, GroupId,
    LaunchIntent, NodeEdit, NodeKind, NodePlace, SessionEdit, SessionId, TerminalId, TerminalNode,
    TextScale,
};
use krad_protocol::{AgentRef, Answer, ErrorCode, EventKind, Reply, RequestId};
use std::path::PathBuf;

use super::working::agent;
use super::{refused, Runtime};
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::launches::WaitingLaunch;

const NOT_IN_A_SESSION: &str = "the terminal to open beside is in no session";
const SHELL_LABEL: &str = "Shell";
const NOT_PLACED: &str = "the new terminal could not be placed in that session";

struct Beside {
    session: SessionId,
    group: Option<GroupId>,
    cwd: Option<PathBuf>,
    id: TerminalId,
}

fn beside_of(data: &EnvironmentData, terminal: TerminalId) -> Option<Beside> {
    let session = data.session_of(terminal)?;
    let group = data.group_of(terminal);
    Some(Beside {
        session,
        group: group.map(|group| group.id),
        cwd: group.map(|group| group.root.clone()),
        id: TerminalId(data.next_id),
    })
}

fn terminal_node(
    id: TerminalId,
    label: &str,
    intent: &LaunchIntent,
    group: Option<GroupId>,
) -> TerminalNode {
    TerminalNode {
        id,
        title: format!("{label} · {}", id.as_u64()),
        state: AgentState::Idle,
        harness: intent.harness(),
        resume: None,
        group,
        text_scale: TextScale::default(),
        kind: NodeKind::Terminal,
        file: None,
        extension: None,
        bookmarks: ChatBookmarks::default(),
        usage: ChatUsage::default(),
        checkpoints: ChatCheckpoints::default(),
        choice: ChatChoice::default(),
        cloud: None,
    }
}

impl Runtime {
    pub(super) fn open_beside(
        &mut self,
        request: RequestId,
        beside: &AgentRef,
        intent: LaunchIntent,
    ) -> Reply {
        let environment = &beside.environment;
        let Some(data) = self.environments.data(environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let Some(placed) = beside_of(data, beside.terminal) else {
            return refused(ErrorCode::NotFound, NOT_IN_A_SESSION);
        };
        let label = intent.harness().map_or_else(
            || SHELL_LABEL.into(),
            |harness| self.settings.current.acp.harness_name(harness),
        );
        let node = terminal_node(placed.id, &label, &intent, placed.group);
        let select = SessionEdit::Select {
            session: placed.session,
        };
        let open = NodeEdit::Open {
            node: Box::new(node),
            place: NodePlace::Tab {
                anchor: Some(beside.terminal),
            },
        };
        for edit in [select.into(), open.into()] {
            self.environments
                .edit(environment.clone(), &edit, None, &mut self.outbox);
        }
        let in_session = self
            .environments
            .data(environment)
            .and_then(|data| data.session_of(placed.id))
            == Some(placed.session);
        if !in_session {
            let close = NodeEdit::Close {
                terminal: placed.id,
            };
            self.environments
                .edit(environment.clone(), &close.into(), None, &mut self.outbox);
            return refused(ErrorCode::Failed, NOT_PLACED);
        }
        let terminal = agent(environment, placed.id);
        self.launch(WaitingLaunch {
            terminal: terminal.clone(),
            intent,
            cwd: placed.cwd,
            prefill: Vec::new(),
        });
        self.outbox.push(EventKind::Answered {
            request,
            answer: Box::new(Answer::TerminalOpened { terminal }),
        });
        Reply::Accepted
    }
}
