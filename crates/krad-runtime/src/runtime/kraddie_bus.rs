use krad_agents::{
    AutomationRunInfo, Caller, ErrorCode, Incoming, KraddieRunInfo, KraddieSharingInfo, Request,
    Response,
};
use krad_domain::{
    AutomationKey, EnvironmentName, KraddieWake, MemoryError, NodeKind, SessionId, Switch,
    TerminalId,
};

use super::working::reply_with;
use super::Runtime;
use crate::kraddies::{automation_info, kraddie_infos, memory_answer, WakeOrder};

const OWN_MEMORY: &str = "A Kraddie always reads its own memory";

const fn memory_code(error: MemoryError) -> ErrorCode {
    match error {
        MemoryError::Empty | MemoryError::TooLong => ErrorCode::BadRequest,
        MemoryError::Full => ErrorCode::Forbidden,
        MemoryError::NoSuchLine => ErrorCode::NotFound,
    }
}

fn no_kraddie(name: &str) -> Response {
    Response::error(ErrorCode::NotFound, format!("No Kraddie named {name}"))
}

const fn is_kraddie_request(request: &Request) -> bool {
    matches!(
        request,
        Request::Kraddies
            | Request::RunKraddie { .. }
            | Request::Automations
            | Request::RunAutomation { .. }
            | Request::SwitchAutomation { .. }
            | Request::RememberKraddie { .. }
            | Request::KraddieMemory { .. }
            | Request::KraddieRecall { .. }
            | Request::ShareKraddieMemory { .. }
            | Request::MemorySearch { .. }
            | Request::MemoryRead { .. }
            | Request::NewKraddieChat { .. }
    )
}

impl Runtime {
    fn caller_terminal(&self, from: &Caller) -> Option<(EnvironmentName, TerminalId)> {
        match from {
            Caller::Terminal(handle) => Some((
                EnvironmentName::new(handle.environment.clone()),
                TerminalId(handle.terminal),
            )),
            Caller::Lead(lead) => {
                let environment = EnvironmentName::new(lead.environment.clone());
                let terminal = self
                    .environments
                    .data(&environment)?
                    .lead(SessionId(lead.session))?
                    .terminal?;
                Some((environment, terminal))
            }
            Caller::Git(_) | Caller::Client => None,
        }
    }

    fn calling_chat(&self, from: &Caller) -> Option<(EnvironmentName, TerminalId)> {
        let (environment, terminal) = self.caller_terminal(from)?;
        let is_chat = self
            .environments
            .data(&environment)?
            .nodes
            .iter()
            .any(|node| node.id == terminal && node.kind == NodeKind::Chat);
        is_chat.then_some((environment, terminal))
    }

    pub(super) fn answer_kraddie_requests(&mut self, incoming: Incoming) -> Option<Incoming> {
        if !is_kraddie_request(&incoming.request) {
            return Some(incoming);
        }
        let reach = self
            .calling_chat(&incoming.from)
            .map(|(environment, chat)| {
                self.kraddies
                    .reach(self.environments.data(&environment), Some(chat))
            });
        if let Some(refusal) = self
            .kraddies
            .memory_refusal(&incoming.request, reach.as_ref())
        {
            reply_with(incoming.reply, Response::Error(refusal));
            return None;
        }
        let answered = matches!(incoming.from, Caller::Terminal(_) | Caller::Lead(_))
            && !matches!(incoming.request, Request::NewKraddieChat { .. });
        if !answered {
            return Some(incoming);
        }
        let response = self.kraddie_answer(&incoming.from, incoming.request);
        reply_with(incoming.reply, response);
        None
    }

    fn kraddie_answer(&mut self, from: &Caller, request: Request) -> Response {
        match request {
            Request::Kraddies => {
                Response::Kraddies(kraddie_infos(&self.kraddies, &self.settings.current.acp))
            }
            Request::Automations => Response::Automations(self.kraddies.automation_infos()),
            Request::RunAutomation { name, prompt } => match self.kraddies.find_automation(&name) {
                Ok(key) => self.run_from_bus(&key, prompt),
                Err(response) => response,
            },
            Request::RunKraddie { name, prompt } => self.run_kraddie_from_bus(&name, prompt),
            Request::SwitchAutomation { name, switch } => self.switch_from_bus(&name, switch),
            Request::RememberKraddie { name, text } => {
                let Some(id) = self.kraddies.kraddie_named(&name) else {
                    return no_kraddie(&name);
                };
                match self.kraddies.remember(&id, &text) {
                    Ok(()) => self.memory_response(&id, &name),
                    Err(error) => Response::error(memory_code(error), error.to_string()),
                }
            }
            Request::KraddieMemory { name } => match self.kraddies.kraddie_named(&name) {
                Some(id) => self.memory_response(&id, &name),
                None => no_kraddie(&name),
            },
            Request::KraddieRecall { name, .. } => self.kraddies.recall(&name),
            Request::ShareKraddieMemory {
                reader,
                from: shared,
                switch,
            } => self.share_from_bus(&reader, &shared, switch),
            request @ (Request::MemorySearch { .. } | Request::MemoryRead { .. }) => {
                let reach = match &self.caller_terminal(from) {
                    Some((environment, terminal)) => self
                        .kraddies
                        .reach(self.environments.data(environment), Some(*terminal)),
                    None => self.kraddies.reach(None, None),
                };
                tracing::debug!(reader = %reach.reader(), "memory asked");
                memory_answer(&self.kraddies.library(), &reach, &request)
            }
            _ => Response::error(ErrorCode::BadRequest, "Not a Kraddie or memory request"),
        }
    }

    fn memory_response(&self, id: &krad_domain::KraddieId, name: &str) -> Response {
        self.kraddies
            .memory_info(id)
            .map_or_else(|| no_kraddie(name), Response::KraddieMemory)
    }

    fn run_from_bus(&mut self, key: &AutomationKey, prompt: Option<String>) -> Response {
        let order = WakeOrder {
            key: key.clone(),
            wake: KraddieWake::Cli,
            prompt,
        };
        let woke = self.wake_kraddie(&order);
        let name = self
            .kraddies
            .state(&key.kraddie)
            .and_then(|state| state.automation(key.automation))
            .map_or_default(|routine| routine.automation.name.to_string());
        match woke {
            Ok(run) => Response::AutomationStarted(AutomationRunInfo {
                kraddie: key.kraddie.to_string(),
                automation: name,
                run: run.0,
            }),
            Err(refusal) => Response::error(refusal.code(), refusal.to_string()),
        }
    }

    fn run_kraddie_from_bus(&mut self, name: &str, prompt: Option<String>) -> Response {
        let Some(id) = self
            .kraddies
            .states()
            .iter()
            .find(|state| state.kraddie.name.matches(name))
            .map(krad_protocol::KraddieState::id)
        else {
            return no_kraddie(name);
        };
        let Some(key) = self.kraddies.default_key(&id) else {
            let refusal = crate::kraddies::KraddieWakeError::NoAutomations;
            return Response::error(refusal.code(), refusal.to_string());
        };
        match self.run_from_bus(&key, prompt) {
            Response::AutomationStarted(started) => Response::KraddieStarted(KraddieRunInfo {
                kraddie: started.kraddie,
                run: started.run,
            }),
            other => other,
        }
    }

    fn switch_from_bus(&mut self, name: &str, switch: Switch) -> Response {
        let key = match self.kraddies.find_automation(name) {
            Ok(key) => key,
            Err(response) => return response,
        };
        drop(self.kraddies.switch_automation(&key, switch));
        let Some(kraddie) = self.kraddies.state(&key.kraddie) else {
            return no_kraddie(name);
        };
        match kraddie.automation(key.automation) {
            Some(routine) => Response::AutomationSwitched(Box::new(automation_info(
                &self.kraddies,
                kraddie,
                routine,
            ))),
            None => Response::error(ErrorCode::NotFound, format!("No automation named {name}")),
        }
    }

    fn share_from_bus(&mut self, reader: &str, from: &str, switch: Switch) -> Response {
        let (Some(reader_id), Some(from_id)) = (
            self.kraddies.kraddie_named(reader),
            self.kraddies.kraddie_named(from),
        ) else {
            return no_kraddie(reader);
        };
        if reader_id == from_id {
            return Response::error(ErrorCode::BadRequest, OWN_MEMORY);
        }
        self.kraddies.share_memory(&reader_id, &from_id, switch);
        let reads = self.kraddies.state(&reader_id).map_or_default(|state| {
            state
                .kraddie
                .settings
                .reads
                .iter()
                .map(ToString::to_string)
                .collect()
        });
        Response::KraddieShared(KraddieSharingInfo {
            kraddie: reader_id.to_string(),
            reads,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_refusals_keep_stable_error_codes() {
        assert_eq!(memory_code(MemoryError::Empty), ErrorCode::BadRequest);
        assert_eq!(memory_code(MemoryError::TooLong), ErrorCode::BadRequest);
        assert_eq!(memory_code(MemoryError::Full), ErrorCode::Forbidden);
        assert_eq!(memory_code(MemoryError::NoSuchLine), ErrorCode::NotFound);
    }
}
