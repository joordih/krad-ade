use krad_domain::ToastKind;
use krad_protocol::{Conversation, HookHeard, HookOrigin};
use std::collections::BTreeSet;

use super::ask::HeldAskReport;
use super::EnvironmentShell;

const FRESH_CONVERSATION: &str = "Fresh conversation";
const FRESH_TOAST: &str = "could not resume its conversation and started a fresh one";

impl EnvironmentShell {
    pub(super) fn hook_heard(&mut self, heard: HookHeard) {
        let HookHeard {
            agent,
            harness,
            event,
            tool,
            mode,
            state,
            activity,
            conversation,
            origin,
        } = heard;
        let (env, id) = (agent.environment, agent.terminal);
        self.mark_terminal_hooked(&env, id);
        if let Some(session) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.sessions.get_mut(&id))
        {
            if state.is_some() {
                session.activity = activity;
            }
            if conversation == Conversation::StartedFresh {
                session.activity = Some(FRESH_CONVERSATION.to_owned());
            }
        }
        if conversation == Conversation::StartedFresh {
            self.push_toast(
                ToastKind::Info,
                format!("terminal #{} {FRESH_TOAST}", id.as_u64()),
            );
        }
        let held = HeldAskReport {
            preset: harness,
            event: &event,
            tool: tool.as_deref(),
            mode,
        };
        self.settle_held_asks(&env, id, &held);
        let Some(state) = state else {
            return;
        };
        let delivered = match origin {
            HookOrigin::Replayed { delivered } => delivered,
            HookOrigin::Live => BTreeSet::new(),
        };
        self.with_webhooks_delivered(delivered, |shell| {
            shell.set_agent_state(&env, id, state);
        });
    }
}
