use futures::channel::oneshot::Sender as ReplySender;
use krad_agents::{Response, TerminalHandle};
use krad_chat::{AskAnswer, AskKind, ToolAsk};
use krad_domain::{
    AgentPreset, EnvironmentName, PermissionMode, SettledAsks, TerminalId, ToolDecision,
};

use super::super::agent_bus::ENTER;
use super::super::surface::Backend;
use super::super::EnvironmentShell;
use super::{AskOwner, AskReply, Waiting, PLAN_AUTO_KEY, PLAN_CHANGE_KEY, PLAN_MANUAL_KEY};

fn plan_keys(answer: &AskAnswer) -> Vec<Vec<u8>> {
    match answer {
        AskAnswer::ApprovePlan {
            mode: PermissionMode::Manual,
        } => vec![PLAN_MANUAL_KEY.to_vec()],
        AskAnswer::ApprovePlan { .. } => vec![PLAN_AUTO_KEY.to_vec()],
        AskAnswer::Deny { message } => vec![
            PLAN_CHANGE_KEY.to_vec(),
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

    pub(super) fn type_plan_answer(&mut self, owner: AskOwner, answer: &AskAnswer) {
        if let AskOwner::Surface(id) = owner {
            let env = self.active.clone();
            self.type_in_steps(&env, id, plan_keys(answer));
        }
    }

    pub(super) fn type_into_owner(&self, owner: AskOwner, keys: &[u8]) {
        if let AskOwner::Surface(id) = owner {
            self.type_into_terminal(&self.active, id, keys.to_vec());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_is_answered_with_the_keys_of_claude_codes_own_dialog() {
        let auto = AskAnswer::ApprovePlan {
            mode: PermissionMode::Auto,
        };
        let manual = AskAnswer::ApprovePlan {
            mode: PermissionMode::Manual,
        };
        let change = AskAnswer::Deny {
            message: "Split it".to_owned(),
        };
        assert_eq!(plan_keys(&auto), [b"1".to_vec()]);
        assert_eq!(plan_keys(&manual), [b"2".to_vec()]);
        assert_eq!(
            plan_keys(&change),
            [b"3".to_vec(), b"Split it".to_vec(), b"\r".to_vec()],
            "the feedback waits for the dialog's field"
        );
        assert_eq!(plan_keys(&AskAnswer::Allow).len(), 0);
    }
}
