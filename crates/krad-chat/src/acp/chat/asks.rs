use krad_domain::{AcpPermissionKind, AcpPermissionOption, ToolCall};
use std::collections::HashMap;
use tracing::debug;

use crate::acp::connection::Responder as Reply;
use crate::acp::permission::{
    PermissionOption, RequestPermissionOutcome, RequestPermissionResponse,
};
use crate::control::{AskAnswer, AskKind, Responder, ToolAsk};

#[derive(Debug, Default)]
pub(super) struct Asks {
    next: u64,
    waiting: HashMap<u64, Reply<RequestPermissionResponse>>,
}

fn option(offered: PermissionOption) -> AcpPermissionOption {
    AcpPermissionOption {
        id: offered.option_id,
        name: offered.name,
        kind: offered.kind,
    }
}

fn first_of(options: &[AcpPermissionOption], kind: AcpPermissionKind) -> RequestPermissionOutcome {
    options.iter().find(|option| option.kind == kind).map_or(
        RequestPermissionOutcome::Cancelled,
        |option| RequestPermissionOutcome::Selected {
            option_id: option.id.clone(),
        },
    )
}

fn outcome(kind: &AskKind, answer: &AskAnswer) -> RequestPermissionOutcome {
    let AskKind::AcpPermission { options, .. } = kind else {
        return RequestPermissionOutcome::Cancelled;
    };
    match answer {
        AskAnswer::Chosen(id) if options.iter().any(|option| &option.id == id) => {
            RequestPermissionOutcome::Selected {
                option_id: id.clone(),
            }
        }
        AskAnswer::Allow => first_of(options, AcpPermissionKind::AllowOnce),
        AskAnswer::AllowAlways => first_of(options, AcpPermissionKind::AllowAlways),
        AskAnswer::Deny { .. } => first_of(options, AcpPermissionKind::RejectOnce),
        AskAnswer::Chosen(_) | AskAnswer::ApprovePlan { .. } | AskAnswer::Answers(_) => {
            RequestPermissionOutcome::Cancelled
        }
    }
}

impl Asks {
    pub(super) fn asked(
        &mut self,
        tool: ToolCall,
        options: Vec<PermissionOption>,
        reply: Reply<RequestPermissionResponse>,
    ) -> ToolAsk {
        let ask = self.next;
        self.next += 1;
        self.waiting.insert(ask, reply);
        ToolAsk {
            responder: Responder::Acp { ask },
            kind: AskKind::AcpPermission {
                tool,
                options: options.into_iter().map(option).collect(),
            },
        }
    }

    pub(super) fn answer(&mut self, ask: &ToolAsk, answer: &AskAnswer) {
        let Responder::Acp { ask: id } = ask.responder else {
            return;
        };
        let Some(reply) = self.waiting.remove(&id) else {
            debug!(
                ask = id,
                "an answer came for an ACP ask that is no longer open"
            );
            return;
        };
        reply.reply(&RequestPermissionResponse {
            outcome: outcome(&ask.kind, answer),
        });
    }

    pub(super) fn cancel_all(&mut self) {
        for (_, reply) in self.waiting.drain() {
            reply.reply(&RequestPermissionResponse {
                outcome: RequestPermissionOutcome::Cancelled,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::AcpPermissionOptionId;

    fn options() -> Vec<AcpPermissionOption> {
        [
            ("once", AcpPermissionKind::AllowOnce),
            ("always", AcpPermissionKind::AllowAlways),
            ("no", AcpPermissionKind::RejectOnce),
        ]
        .into_iter()
        .map(|(id, kind)| AcpPermissionOption {
            id: AcpPermissionOptionId(id.to_owned()),
            name: id.to_owned(),
            kind,
        })
        .collect()
    }

    fn kind() -> AskKind {
        AskKind::AcpPermission {
            tool: ToolCall {
                id: None,
                name: "write".to_owned(),
                kind: krad_domain::ToolKind::Edit,
                summary: String::new(),
                edit: None,
            },
            options: options(),
        }
    }

    fn selected(id: &str) -> RequestPermissionOutcome {
        RequestPermissionOutcome::Selected {
            option_id: AcpPermissionOptionId(id.to_owned()),
        }
    }

    #[test]
    fn a_chosen_option_is_sent_back_and_the_usual_answers_pick_the_matching_kind() {
        let chosen = AskAnswer::Chosen(AcpPermissionOptionId("always".to_owned()));
        assert_eq!(outcome(&kind(), &chosen), selected("always"));
        assert_eq!(outcome(&kind(), &AskAnswer::Allow), selected("once"));
        assert_eq!(
            outcome(
                &kind(),
                &AskAnswer::Deny {
                    message: String::new()
                }
            ),
            selected("no")
        );
    }

    #[test]
    fn an_option_the_agent_did_not_offer_cancels() {
        let foreign = AskAnswer::Chosen(AcpPermissionOptionId("other".to_owned()));
        assert_eq!(
            outcome(&kind(), &foreign),
            RequestPermissionOutcome::Cancelled
        );
        assert_eq!(
            outcome(
                &AskKind::Plan {
                    plan: String::new()
                },
                &AskAnswer::Allow
            ),
            RequestPermissionOutcome::Cancelled
        );
    }
}
