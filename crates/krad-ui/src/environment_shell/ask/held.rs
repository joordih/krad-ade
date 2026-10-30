use futures::channel::oneshot::Sender as ReplySender;
use krad_agents::{Response, TerminalHandle};
use krad_chat::{AskAnswer, AskKind, ToolAsk};
use krad_domain::{
    AgentPreset, EnvironmentName, PermissionMode, PlanDialogKeys, SettledAsks, TerminalId,
    ToolDecision,
};

use super::super::agent_bus::ENTER;
use super::super::surface::Backend;
use super::super::EnvironmentShell;
use super::{AskReply, Waiting};

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
        AskAnswer::Allow | AskAnswer::AllowAlways | AskAnswer::Answers(_) => Vec::new(),
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn hold_ask(
        &mut self,
        from: &TerminalHandle,
        ask: ToolAsk,
        tool: String,
        reply: ReplySender<Response>,
    ) {
        let env = EnvironmentName::new(from.environment.clone());
        let chat = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&TerminalId(from.terminal)))
            .filter(|chat| matches!(chat.backend, Backend::Transcript(_)));
        let Some(chat) = chat else {
            reply
                .send(Response::Decided(ToolDecision::Pass))
                .unwrap_or_default();
            return;
        };
        let reply = if matches!(ask.kind, AskKind::Plan { .. }) {
            reply
                .send(Response::Decided(ToolDecision::Pass))
                .unwrap_or_default();
            AskReply::Keys
        } else {
            AskReply::Hook(reply)
        };
        chat.asks_mut().push(Waiting::held(ask, reply, tool));
    }

    pub(in crate::environment_shell) fn settle_held_asks(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        preset: AgentPreset,
        event: &str,
        tool: Option<&str>,
    ) {
        let Some(settled) = preset.settled_asks(event) else {
            return;
        };
        if let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            let tool = match settled {
                SettledAsks::OfTool => tool,
                SettledAsks::All => None,
            };
            chat.asks_mut().settle(tool);
        }
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
