use futures::channel::oneshot;
use krad_agents::{CloseOrder, CloseTarget, LeadHandle};
use krad_domain::CloseRefusal;
use krad_protocol::{AgentAct, AgentCommand, ErrorCode, Reply};

use super::working::LeadClose;
use super::{refused, Runtime, UNSUPPORTED_COMMAND};
use crate::environments::UNKNOWN_ENVIRONMENT;

const NOT_A_LEAD: &str = "this session has no lead that can start this worker";
const UNKNOWN_WORKER: &str = "no open worker of this lead with that id";
const NOT_OWNED: &str = "this worker belongs to another lead";
const ITSELF: &str = "a lead cannot close itself as a worker";
const WORKING: &str = "the worker is working; cancel its turn to close it";

const fn close_refusal(refusal: CloseRefusal) -> (ErrorCode, &'static str) {
    match refusal {
        CloseRefusal::Unknown | CloseRefusal::AlreadyClosed => {
            (ErrorCode::NotFound, UNKNOWN_WORKER)
        }
        CloseRefusal::NotOwned => (ErrorCode::Forbidden, NOT_OWNED),
        CloseRefusal::Itself => (ErrorCode::Forbidden, ITSELF),
        CloseRefusal::Working => (ErrorCode::Busy, WORKING),
    }
}

impl Runtime {
    pub(super) fn execute_agent(&mut self, command: AgentCommand) -> Reply {
        let AgentCommand {
            environment,
            lead,
            act,
        } = command;
        if self.environments.data(&environment).is_none() {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        }
        let handle = LeadHandle {
            environment: environment.as_str().to_owned(),
            session: lead.as_u64(),
        };
        let (reply, _unheard) = oneshot::channel();
        match act {
            AgentAct::StartWorker {
                agent,
                worktree,
                task,
            } => match self.start_worker(&handle, (agent, worktree.as_deref(), &task), reply) {
                Ok(()) => Reply::Accepted,
                Err(_) => refused(ErrorCode::Forbidden, NOT_A_LEAD),
            },
            AgentAct::CloseWorker {
                worker,
                when_working,
                worktree,
            } => {
                let order = CloseOrder {
                    target: CloseTarget::Agent(worker.as_u64()),
                    when_working,
                    worktree,
                };
                match self.close_as_lead(&handle, order) {
                    LeadClose::Removing(plans) => {
                        self.close_removing(&environment, &plans, reply);
                        Reply::Accepted
                    }
                    LeadClose::Closed(_) => Reply::Accepted,
                    LeadClose::Refused(refusal) => {
                        let (code, message) = close_refusal(refusal);
                        refused(code, message)
                    }
                }
            }
            AgentAct::Unknown => refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND),
        }
    }
}
