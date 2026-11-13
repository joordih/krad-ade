use krad_agents::{Caller, ErrorCode, KraddieChatInfo, Response};
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    EnvironmentName, GroupId, KraddieId, NodeEdit, NodePlace, SessionEdit, SessionId, TerminalId,
};
use krad_protocol::Reply;

use super::working::agent;
use super::{refused, Runtime};
use crate::chats::{ChatBrief, ChatRole, ChatStart};
use crate::workers::{runs_as_worker, worker_node};

const CHAT_NAME_SUFFIX: &str = " chat";
const NOT_A_CHAT_KRADDIE: &str = "This Kraddie's agent cannot open a chat; pick another in Tuning";
const NO_ENVIRONMENT: &str = "No environment is open for the chat";
const NO_KRADDIE: &str = "That Kraddie is no longer here";
const NOT_OPENED: &str = "The Kraddie's chat could not open";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChatRefusal {
    NoKraddie,
    NotAChat,
    NoEnvironment,
    NotOpened,
}

impl ChatRefusal {
    const fn text(self) -> &'static str {
        match self {
            Self::NoKraddie => NO_KRADDIE,
            Self::NotAChat => NOT_A_CHAT_KRADDIE,
            Self::NoEnvironment => NO_ENVIRONMENT,
            Self::NotOpened => NOT_OPENED,
        }
    }

    const fn code(self) -> krad_protocol::ErrorCode {
        match self {
            Self::NoKraddie | Self::NoEnvironment => krad_protocol::ErrorCode::NotFound,
            Self::NotAChat => krad_protocol::ErrorCode::Unsupported,
            Self::NotOpened => krad_protocol::ErrorCode::Unavailable,
        }
    }

    const fn bus_code(self) -> ErrorCode {
        match self {
            Self::NoKraddie | Self::NoEnvironment => ErrorCode::NotFound,
            Self::NotAChat | Self::NotOpened => ErrorCode::Unavailable,
        }
    }
}

struct OpenedChat {
    session: SessionId,
    terminal: TerminalId,
}

impl Runtime {
    pub(super) fn open_kraddie_chat_for(
        &mut self,
        kraddie: &KraddieId,
        environment: &EnvironmentName,
    ) -> Reply {
        match self.open_kraddie_chat(kraddie, environment) {
            Ok(_) => Reply::Accepted,
            Err(refusal) => refused(refusal.code(), refusal.text()),
        }
    }

    pub(super) fn kraddie_chat_from_bus(&mut self, from: &Caller, name: &str) -> Response {
        let Some(kraddie) = self.kraddies.kraddie_named(name) else {
            return Response::error(ErrorCode::NotFound, format!("No Kraddie named {name}"));
        };
        let environment = match from {
            Caller::Terminal(terminal) => Some(EnvironmentName::new(terminal.environment.clone())),
            Caller::Lead(lead) => Some(EnvironmentName::new(lead.environment.clone())),
            Caller::Git(_) | Caller::Client => self
                .kraddies
                .state(&kraddie)
                .and_then(|state| state.kraddie.workspace.as_ref())
                .map(|workspace| workspace.environment.clone()),
        };
        let opened = environment
            .ok_or(ChatRefusal::NoEnvironment)
            .and_then(|environment| self.open_kraddie_chat(&kraddie, &environment));
        match opened {
            Ok(opened) => Response::KraddieChatOpened(KraddieChatInfo {
                kraddie: kraddie.to_string(),
                session: opened.session.as_u64(),
                terminal: opened.terminal.as_u64(),
            }),
            Err(refusal) => Response::error(refusal.bus_code(), refusal.text()),
        }
    }

    fn kraddie_chat_group(
        &mut self,
        kraddie: &KraddieId,
        environment: &EnvironmentName,
    ) -> Option<GroupId> {
        let own = self
            .kraddies
            .state(kraddie)
            .and_then(|state| state.kraddie.workspace.clone())
            .filter(|place| place.environment == *environment);
        if let Some(place) = own {
            return self.group_for(environment, &place.repo);
        }
        let data = self.environments.data(environment)?;
        data.active_session
            .and_then(|session| data.session(session))
            .and_then(|session| session.group)
            .or_else(|| data.groups.first().map(|group| group.id))
    }

    fn open_kraddie_chat(
        &mut self,
        kraddie: &KraddieId,
        environment: &EnvironmentName,
    ) -> Result<OpenedChat, ChatRefusal> {
        let state = self.kraddies.state(kraddie).ok_or(ChatRefusal::NoKraddie)?;
        let (name, harness) = (state.kraddie.name.to_string(), state.kraddie.harness);
        let settings = state.kraddie.settings.clone();
        let model = settings
            .session_model
            .clone()
            .or(state.kraddie.model.clone());
        if !runs_as_worker(harness) {
            return Err(ChatRefusal::NotAChat);
        }
        if !self.environments.knows(environment) {
            return Err(ChatRefusal::NoEnvironment);
        }
        let group = self.kraddie_chat_group(kraddie, environment);
        let data = self
            .environments
            .data(environment)
            .ok_or(ChatRefusal::NoEnvironment)?;
        let session = data.next_session_id();
        let terminal = TerminalId(data.next_id);
        let opened = SessionEdit::OpenForKraddie {
            session,
            group,
            kraddie: kraddie.clone(),
            name: format!("{name}{CHAT_NAME_SUFFIX}"),
        };
        self.environments
            .edit(environment.clone(), &opened.into(), None, &mut self.outbox);
        let label = self.settings.current.acp.harness_name(harness);
        let node = worker_node(terminal, &label, harness, group);
        let placed = NodeEdit::Open {
            node: Box::new(node),
            place: NodePlace::Tab { anchor: None },
        };
        self.environments
            .edit(environment.clone(), &placed.into(), None, &mut self.outbox);
        let in_session = self
            .environments
            .data(environment)
            .and_then(|data| data.session_of(terminal))
            == Some(session);
        if !in_session {
            return Err(ChatRefusal::NotOpened);
        }
        let start = ChatStart {
            harness,
            brief: ChatBrief {
                model,
                effort: settings.effort,
                permission_mode: settings.permissions,
                role: ChatRole::Agent,
                ..ChatBrief::default()
            },
            prepared: None,
            session: LiveSession::Start(new_session_id()),
            messages: Vec::new(),
            queued: Vec::new(),
            notices: Vec::new(),
        };
        self.open_chat(&agent(environment, terminal), start);
        tracing::info!(kraddie = %kraddie, session = session.as_u64(), "kraddie chat opened");
        Ok(OpenedChat { session, terminal })
    }
}
