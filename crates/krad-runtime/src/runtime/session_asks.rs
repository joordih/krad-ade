use krad_agents::{Caller, ErrorCode, Incoming, Request, Response};
use krad_domain::agents_api::{session_infos, target_group, ApiError};
use krad_domain::{
    ClosePlan, CloseSubject, EnvironmentName, GroupId, SessionEdit, SessionId, TerminalId,
};

use super::working::reply_with;
use super::Runtime;

const NO_ENVIRONMENT: &str = "The caller's environment is not open";
const NO_SUCH_SESSION: &str = "No session with that id in your environment";

struct Asker {
    environment: EnvironmentName,
    group: Option<GroupId>,
}

fn no_environment() -> ApiError {
    ApiError::new(ErrorCode::NotFound, NO_ENVIRONMENT)
}

const fn asks_sessions(request: &Request) -> bool {
    matches!(
        request,
        Request::Sessions | Request::CreateSession { .. } | Request::CloseSession { .. }
    )
}

impl Runtime {
    pub(super) fn answer_session_asks(&mut self, incoming: Incoming) -> Option<Incoming> {
        if !asks_sessions(&incoming.request) {
            return Some(incoming);
        }
        let Some(asker) = self.session_asker(&incoming.from, &incoming.request) else {
            return Some(incoming);
        };
        let Incoming { request, reply, .. } = incoming;
        let answered = match request {
            Request::Sessions => self
                .environments
                .data(&asker.environment)
                .map(|data| Response::Sessions(session_infos(data)))
                .ok_or_else(no_environment),
            Request::CreateSession { name, group } => {
                self.create_session_for(&asker, group.as_deref(), name)
            }
            Request::CloseSession { session } => {
                self.close_session_for(&asker.environment, SessionId(session))
            }
            _ => return None,
        };
        reply_with(reply, answered.unwrap_or_else(Response::Error));
        None
    }

    fn session_asker(&self, from: &Caller, request: &Request) -> Option<Asker> {
        match from {
            Caller::Terminal(terminal) => {
                let environment = EnvironmentName::new(terminal.environment.clone());
                let group = self
                    .environments
                    .data(&environment)
                    .and_then(|data| data.group_of(TerminalId(terminal.terminal)))
                    .map(|group| group.id);
                Some(Asker { environment, group })
            }
            Caller::Lead(lead) if !matches!(request, Request::CloseSession { .. }) => {
                let environment = EnvironmentName::new(lead.environment.clone());
                let session = self
                    .environments
                    .data(&environment)?
                    .session(SessionId(lead.session))?;
                session.lead.as_ref()?;
                let group = Some(session.group?);
                Some(Asker { environment, group })
            }
            Caller::Lead(_) | Caller::Git(_) | Caller::Client => None,
        }
    }

    fn create_session_for(
        &mut self,
        asker: &Asker,
        group: Option<&str>,
        name: Option<String>,
    ) -> Result<Response, ApiError> {
        let data = self
            .environments
            .data(&asker.environment)
            .ok_or_else(no_environment)?;
        let group = target_group(data, asker.group, group)?;
        let session = data.next_session_id();
        let opened = SessionEdit::Open {
            session,
            group: Some(group),
            name: name.unwrap_or_default(),
            shown: false,
        };
        self.environments.edit(
            asker.environment.clone(),
            &opened.into(),
            None,
            &mut self.outbox,
        );
        Ok(Response::SessionCreated {
            session: session.as_u64(),
        })
    }

    fn close_session_for(
        &mut self,
        environment: &EnvironmentName,
        session: SessionId,
    ) -> Result<Response, ApiError> {
        let data = self
            .environments
            .data(environment)
            .ok_or_else(no_environment)?;
        let closing = data
            .session(session)
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, NO_SUCH_SESSION))?;
        let mut terminals = data.open_workers(session);
        terminals.extend(
            closing
                .tabs
                .tabs
                .iter()
                .flat_map(|tab| tab.tiles.iter().map(|tile| tile.id)),
        );
        let cancel = terminals
            .iter()
            .copied()
            .filter(|terminal| self.is_busy(environment, *terminal))
            .collect();
        let plan = ClosePlan {
            subject: CloseSubject::Session(session),
            terminals,
            cancel,
            worktree: None,
        };
        self.close_planned(environment, &plan);
        Ok(Response::SessionClosed {
            session: session.as_u64(),
        })
    }
}
