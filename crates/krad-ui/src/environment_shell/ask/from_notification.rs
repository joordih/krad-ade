use gpui_kit::Context;
use krad_chat::{AskAnswer, AskKind};
use krad_domain::{AcpPermissionKind, AttentionKind, EnvironmentName, TerminalId};

use super::super::surface::SurfaceChat;
use super::super::EnvironmentShell;
use super::{subject, AskQueue, DENIED_TEXT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum PermissionReply {
    Approve,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct FrontAsk {
    pub(in crate::environment_shell) kind: AttentionKind,
    pub(in crate::environment_shell) subject: String,
}

const fn attention_kind(kind: &AskKind) -> AttentionKind {
    match kind {
        AskKind::Permission { .. } | AskKind::AcpPermission { .. } => AttentionKind::Permission,
        AskKind::Questions(_) | AskKind::Plan { .. } => AttentionKind::Ask,
    }
}

fn permission_answer(kind: &AskKind, reply: PermissionReply) -> Option<AskAnswer> {
    match (kind, reply) {
        (AskKind::Permission { .. }, PermissionReply::Approve) => Some(AskAnswer::Allow),
        (AskKind::Permission { .. }, PermissionReply::Deny) => Some(AskAnswer::Deny {
            message: DENIED_TEXT.to_owned(),
        }),
        (AskKind::AcpPermission { options, .. }, reply) => {
            let wanted = match reply {
                PermissionReply::Approve => AcpPermissionKind::AllowOnce,
                PermissionReply::Deny => AcpPermissionKind::RejectOnce,
            };
            options
                .iter()
                .find(|option| option.kind == wanted)
                .map(|option| AskAnswer::Chosen(option.id.clone()))
        }
        (AskKind::Questions(_) | AskKind::Plan { .. }, _) => None,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn front_ask(
        &self,
        env: &EnvironmentName,
        owner: TerminalId,
    ) -> Option<FrontAsk> {
        let waiting = self
            .envs
            .get(env)?
            .chats
            .get(&owner)
            .map(SurfaceChat::asks)
            .and_then(AskQueue::front)?;
        Some(FrontAsk {
            kind: attention_kind(&waiting.ask.kind),
            subject: subject(&waiting.ask.kind),
        })
    }

    pub(in crate::environment_shell) fn reply_to_permission(
        &mut self,
        owner: TerminalId,
        reply: PermissionReply,
        cx: &mut Context<Self>,
    ) -> bool {
        let answer = self
            .ask_queue(owner)
            .and_then(AskQueue::front)
            .and_then(|waiting| permission_answer(&waiting.ask.kind, reply));
        let Some(answer) = answer else {
            return false;
        };
        self.answer_ask(owner, &answer, cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AcpPermissionOption, AcpPermissionOptionId, ToolCall, ToolKind};

    fn permission() -> AskKind {
        AskKind::Permission {
            tool: "Bash".to_owned(),
            kind: ToolKind::of_claude_code("Bash"),
            summary: "cargo test".to_owned(),
            title: None,
            edit: None,
        }
    }

    fn chosen(id: &str) -> AskAnswer {
        AskAnswer::Chosen(AcpPermissionOptionId(id.to_owned()))
    }

    #[test]
    fn a_permission_is_approved_or_denied_from_its_reply() {
        assert_eq!(
            permission_answer(&permission(), PermissionReply::Approve),
            Some(AskAnswer::Allow)
        );
        assert_eq!(
            permission_answer(&permission(), PermissionReply::Deny),
            Some(AskAnswer::Deny {
                message: DENIED_TEXT.to_owned(),
            })
        );
    }

    #[test]
    fn questions_and_plans_cannot_be_answered_from_a_notification() {
        let plan = AskKind::Plan {
            plan: "# Plan".to_owned(),
        };
        let questions = AskKind::Questions(Vec::new());
        for kind in [plan, questions] {
            assert_eq!(attention_kind(&kind), AttentionKind::Ask);
            assert_eq!(permission_answer(&kind, PermissionReply::Approve), None);
        }
        assert_eq!(attention_kind(&permission()), AttentionKind::Permission);
    }

    #[test]
    fn an_agent_permission_picks_its_allow_once_or_reject_once_option() {
        let option = |id: &str, kind| AcpPermissionOption {
            id: AcpPermissionOptionId(id.to_owned()),
            name: id.to_owned(),
            kind,
        };
        let kind = AskKind::AcpPermission {
            tool: ToolCall {
                id: None,
                name: "cargo test".to_owned(),
                kind: ToolKind::Shell,
                summary: String::new(),
                edits: Vec::new(),
            },
            options: vec![
                option("always", AcpPermissionKind::AllowAlways),
                option("once", AcpPermissionKind::AllowOnce),
                option("reject", AcpPermissionKind::RejectOnce),
            ],
        };
        assert_eq!(attention_kind(&kind), AttentionKind::Permission);
        assert_eq!(
            permission_answer(&kind, PermissionReply::Approve),
            Some(chosen("once"))
        );
        assert_eq!(
            permission_answer(&kind, PermissionReply::Deny),
            Some(chosen("reject"))
        );
    }
}
