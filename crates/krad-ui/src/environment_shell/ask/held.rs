use krad_chat::{AskAnswer, AskKind, ToolAsk};
use krad_domain::{
    AgentPreset, EnvironmentName, PermissionMode, PlanDialogKeys, SettledAsks, TerminalId,
    ToolDecision,
};
use krad_protocol::{AskId, AskRequest, PendingAsk};

use super::super::agent_bus::ENTER;
use super::super::surface::Backend;
use super::super::EnvironmentShell;
use super::{AskReply, Settled, Waiting};

fn plan_keys(keys: PlanDialogKeys, answer: &AskAnswer) -> Vec<Vec<u8>> {
    match answer {
        AskAnswer::ApprovePlan {
            mode: PermissionMode::Manual,
        } => vec![keys.approve_manually.to_vec()],
        AskAnswer::ApprovePlan { .. } => vec![keys.approve.to_vec()],
        AskAnswer::Deny { message } => vec![
            keys.keep_planning.to_vec(),
            message.as_bytes().to_vec(),
            ENTER.to_vec(),
        ],
        AskAnswer::Allow
        | AskAnswer::AllowAlways
        | AskAnswer::Answers(_)
        | AskAnswer::Chosen(_) => Vec::new(),
    }
}

pub(in crate::environment_shell) struct HeldAskReport<'a> {
    pub(in crate::environment_shell) preset: AgentPreset,
    pub(in crate::environment_shell) event: &'a str,
    pub(in crate::environment_shell) tool: Option<&'a str>,
    pub(in crate::environment_shell) mode: Option<PermissionMode>,
}

impl HeldAskReport<'_> {
    fn left_plan(&self) -> bool {
        self.mode.is_some_and(|mode| mode != PermissionMode::Plan)
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn hold_ask(&mut self, pending: PendingAsk) {
        let PendingAsk {
            ask,
            agent,
            request:
                AskRequest {
                    tool,
                    input,
                    suggestions,
                    call,
                },
        } = pending;
        let held = self.runtime.handle.held_ask(ask);
        let asked = ToolAsk::held(tool.clone(), input, suggestions);
        let chat = self
            .envs
            .get_mut(&agent.environment)
            .and_then(|live| live.chats.get_mut(&agent.terminal))
            .filter(|chat| matches!(chat.backend, Backend::Transcript(_)));
        let Some(chat) = chat else {
            held.decide(ToolDecision::Pass);
            return;
        };
        let reply = if matches!(asked.kind, AskKind::Plan { .. }) {
            held.decide(ToolDecision::Pass);
            AskReply::Keys
        } else {
            AskReply::Hook(held)
        };
        chat.asks_mut()
            .push(Waiting::held(asked, reply, tool, call));
        self.raise_front_ask(&agent.environment, agent.terminal);
    }

    pub(in crate::environment_shell) fn forget_held_ask(&mut self, ask: AskId) -> bool {
        let mut forgotten = false;
        for live in self.envs.values_mut() {
            for chat in live.chats.values_mut() {
                forgotten |= chat.asks_mut().forget_ask(ask);
            }
        }
        forgotten
    }

    pub(in crate::environment_shell) fn settle_held_asks(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        report: &HeldAskReport<'_>,
    ) {
        let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
        else {
            return;
        };
        let asks = chat.asks_mut();
        if report.left_plan() {
            asks.settle(Settled::Plans);
        }
        let settled = match (report.preset.settled_asks(report.event), report.tool) {
            (Some(SettledAsks::All), _) => Settled::All,
            (Some(SettledAsks::OfTool), Some(tool)) => Settled::OfTool(tool),
            (Some(SettledAsks::PastTool), Some(tool)) => Settled::PastTool(tool),
            (Some(SettledAsks::OfTool | SettledAsks::PastTool) | None, _) => return,
        };
        asks.settle(settled);
    }

    pub(super) fn type_plan_answer(&mut self, owner: TerminalId, answer: &AskAnswer) {
        let env = self.active.clone();
        let keys = self
            .envs
            .get(&env)
            .and_then(|live| live.chats.get(&owner))
            .and_then(|chat| chat.harness().plan_dialog_keys());
        let Some(keys) = keys else {
            return;
        };
        self.type_in_steps(&env, owner, plan_keys(keys, answer));
    }

    pub(super) fn type_into_owner(&self, owner: TerminalId, keys: &[u8]) {
        self.type_into_terminal(&self.active, owner, keys.to_vec());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_is_answered_with_the_keys_of_claude_codes_own_dialog() {
        let Some(claude) = AgentPreset::ClaudeCode.plan_dialog_keys() else {
            unreachable!("claude code answers its plan dialog by keys");
        };
        let auto = AskAnswer::ApprovePlan {
            mode: PermissionMode::Auto,
        };
        let manual = AskAnswer::ApprovePlan {
            mode: PermissionMode::Manual,
        };
        let change = AskAnswer::Deny {
            message: "Split it".to_owned(),
        };
        assert_eq!(plan_keys(claude, &auto), [b"1".to_vec()]);
        assert_eq!(plan_keys(claude, &manual), [b"2".to_vec()]);
        assert_eq!(
            plan_keys(claude, &change),
            [b"3".to_vec(), b"Split it".to_vec(), b"\r".to_vec()],
            "the feedback waits for the dialog's field"
        );
        assert_eq!(plan_keys(claude, &AskAnswer::Allow).len(), 0);
    }
}
