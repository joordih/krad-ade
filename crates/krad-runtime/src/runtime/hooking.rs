use krad_agents::{Request, Response, TerminalHandle};
use krad_domain::{hook_state, EnvironmentName, NodeEdit, NodeField, ResumeId, TerminalId};
use krad_protocol::{AgentRef, Conversation, EventKind, HookHeard, HookOrigin, HookReport};

use super::Runtime;

pub(super) fn hook_report(request: Request) -> Option<HookReport> {
    match request {
        Request::Hook {
            harness,
            event,
            tool,
            detail,
            resume,
            mode,
        } => Some(HookReport {
            harness,
            event,
            tool,
            detail,
            resume,
            mode,
        }),
        _ => None,
    }
}

pub(super) fn agent_of(from: &TerminalHandle) -> AgentRef {
    AgentRef {
        environment: EnvironmentName::new(from.environment.clone()),
        terminal: TerminalId(from.terminal),
    }
}

impl Runtime {
    #[tracing::instrument(skip_all, fields(environment = agent.environment.as_str(), terminal = agent.terminal.as_u64()))]
    pub(super) fn hear_hook(
        &mut self,
        agent: AgentRef,
        report: HookReport,
        origin: HookOrigin,
    ) -> Response {
        let HookReport {
            harness,
            event,
            tool,
            detail,
            resume,
            mode,
        } = report;
        let state = hook_state(harness, &event, tool.as_deref());
        if origin == HookOrigin::Live {
            self.note_live_cloud_report(&agent);
        }
        self.terminals.mark_hooked(&agent);
        let fresh = harness.started_fresh(&event, tool.as_deref());
        let resume = resume.and_then(|raw| raw.parse::<ResumeId>().ok());
        let conversation = self.keep_hooked_resume(&agent, harness, resume, fresh);
        self.outbox.push(EventKind::AgentHooked(HookHeard {
            agent,
            harness,
            event,
            tool,
            mode,
            state,
            activity: state.and(detail),
            conversation,
            origin,
        }));
        Response::Noted
    }

    fn keep_hooked_resume(
        &mut self,
        agent: &AgentRef,
        harness: krad_domain::AgentPreset,
        resume: Option<ResumeId>,
        fresh: bool,
    ) -> Conversation {
        let Some(kept) = self
            .environments
            .data(&agent.environment)
            .and_then(|data| data.nodes.iter().find(|node| node.id == agent.terminal))
            .map(|node| node.resume.clone())
        else {
            return Conversation::Kept;
        };
        let Some(resume) = resume.filter(|resume| kept.as_ref() != Some(resume)) else {
            return Conversation::Kept;
        };
        for field in [
            NodeField::Resume(Some(resume)),
            NodeField::Harness(Some(harness)),
        ] {
            let edit = NodeEdit::Set {
                terminal: agent.terminal,
                field,
            };
            self.environments.edit(
                agent.environment.clone(),
                &edit.into(),
                None,
                &mut self.outbox,
            );
        }
        if fresh && kept.is_some() {
            Conversation::StartedFresh
        } else {
            Conversation::Kept
        }
    }
}
