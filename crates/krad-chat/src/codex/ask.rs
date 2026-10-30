use krad_domain::{ToolCall, ToolKind};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

use super::events::{BASH_TOOL, EDIT_TOOL};
use super::rpc::RpcId;
use crate::control::{AskAnswer, AskKind, Question, QuestionOption, Responder, ToolAsk};

const COMMAND_APPROVAL: &str = "item/commandExecution/requestApproval";
const FILE_APPROVAL: &str = "item/fileChange/requestApproval";
const USER_INPUT: &str = "item/tool/requestUserInput";
const PERMISSIONS_APPROVAL: &str = "item/permissions/requestApproval";
const PERMISSIONS_TOOL: &str = "Permissions";
const ACCEPT: &str = "accept";
const ACCEPT_FOR_SESSION: &str = "acceptForSession";
const DECLINE: &str = "decline";
const CANCEL: &str = "cancel";
const TURN_SCOPE: &str = "turn";
const SESSION_SCOPE: &str = "session";
const PATH_SEPARATOR: &str = ", ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CodexAsk {
    Command {
        amendment: Option<Vec<String>>,
        for_session: bool,
        declinable: bool,
    },
    FileChange,
    UserInput {
        questions: Vec<String>,
    },
    Permissions {
        requested: Value,
    },
}

#[derive(Debug, Deserialize)]
struct ActionCommand {
    command: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandApproval {
    command: Option<String>,
    reason: Option<String>,
    #[serde(default)]
    command_actions: Vec<ActionCommand>,
    proposed_execpolicy_amendment: Option<Vec<String>>,
    #[serde(default)]
    available_decisions: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileApproval {
    item_id: String,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InputOption {
    label: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InputQuestion {
    id: String,
    header: Option<String>,
    question: String,
    #[serde(default)]
    options: Option<Vec<InputOption>>,
}

#[derive(Debug, Deserialize)]
struct UserInputRequest {
    questions: Vec<InputQuestion>,
}

#[derive(Debug, Deserialize)]
struct PermissionsRequest {
    reason: Option<String>,
    #[serde(default)]
    permissions: Value,
}

impl CodexAsk {
    pub(crate) const fn can_allow_always(&self) -> bool {
        match self {
            Self::Command {
                amendment,
                for_session,
                ..
            } => *for_session || amendment.is_some(),
            Self::FileChange | Self::Permissions { .. } => true,
            Self::UserInput { .. } => false,
        }
    }
}

const fn ask(id: RpcId, kind: AskKind, codex: CodexAsk) -> ToolAsk {
    ToolAsk {
        responder: Responder::Codex { id, ask: codex },
        kind,
    }
}

fn offers(decisions: &[Value], decision: &str) -> bool {
    decisions
        .iter()
        .any(|offered| offered.as_str() == Some(decision))
}

pub(crate) fn server_ask(
    id: RpcId,
    method: &str,
    params: &Value,
    changes: &HashMap<String, Vec<ToolCall>>,
) -> Option<ToolAsk> {
    match method {
        COMMAND_APPROVAL => {
            let asked = CommandApproval::deserialize(params).ok()?;
            let summary = asked
                .command_actions
                .first()
                .and_then(|action| action.command.clone())
                .or(asked.command)
                .unwrap_or_default();
            let for_session = offers(&asked.available_decisions, ACCEPT_FOR_SESSION);
            let codex = CodexAsk::Command {
                declinable: offers(&asked.available_decisions, DECLINE),
                amendment: asked.proposed_execpolicy_amendment.filter(|_| !for_session),
                for_session,
            };
            let (tool, kind) = BASH_TOOL;
            let kind = AskKind::Permission {
                tool: tool.to_owned(),
                kind,
                summary,
                title: asked.reason,
                edit: None,
            };
            Some(ask(id, kind, codex))
        }
        FILE_APPROVAL => {
            let asked = FileApproval::deserialize(params).ok()?;
            let tools = changes.get(&asked.item_id).map_or(&[][..], Vec::as_slice);
            let (tool, kind) = tools.first().map_or_else(
                || (EDIT_TOOL.0.to_owned(), EDIT_TOOL.1),
                |tool| (tool.name.clone(), tool.kind),
            );
            let kind = AskKind::Permission {
                tool,
                kind,
                summary: tools
                    .iter()
                    .map(|tool| tool.summary.as_str())
                    .collect::<Vec<_>>()
                    .join(PATH_SEPARATOR),
                title: asked.reason,
                edit: tools.iter().find_map(|tool| tool.edit.clone()),
            };
            Some(ask(id, kind, CodexAsk::FileChange))
        }
        USER_INPUT => {
            let asked = UserInputRequest::deserialize(params).ok()?;
            let codex = CodexAsk::UserInput {
                questions: asked
                    .questions
                    .iter()
                    .map(|question| question.id.clone())
                    .collect(),
            };
            let questions = asked
                .questions
                .into_iter()
                .map(|question| Question {
                    text: question.question,
                    header: question.header,
                    options: question
                        .options
                        .unwrap_or_default()
                        .into_iter()
                        .map(|option| QuestionOption {
                            label: option.label,
                            description: option.description,
                        })
                        .collect(),
                    multi_select: false,
                })
                .collect();
            Some(ask(id, AskKind::Questions(questions), codex))
        }
        PERMISSIONS_APPROVAL => {
            let asked = PermissionsRequest::deserialize(params).ok()?;
            let kind = AskKind::Permission {
                tool: PERMISSIONS_TOOL.to_owned(),
                kind: ToolKind::Other,
                summary: asked.reason.clone().unwrap_or_default(),
                title: asked.reason,
                edit: None,
            };
            let codex = CodexAsk::Permissions {
                requested: asked.permissions,
            };
            Some(ask(id, kind, codex))
        }
        _ => None,
    }
}

#[derive(Serialize)]
struct Amendment<'a> {
    execpolicy_amendment: &'a [String],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum AmendedDecision<'a> {
    AcceptWithExecpolicyAmendment(Amendment<'a>),
}

#[derive(Serialize)]
struct Decision<D> {
    decision: D,
}

#[derive(Serialize)]
struct Answered {
    answers: Vec<String>,
}

#[derive(Serialize)]
struct Answers {
    answers: Map<String, Value>,
}

#[derive(Serialize)]
struct Granted<'a> {
    permissions: &'a Value,
    scope: &'static str,
}

fn to_body(body: &impl Serialize) -> Value {
    serde_json::to_value(body).unwrap_or_default()
}

pub(crate) fn reply(codex: &CodexAsk, answer: &AskAnswer) -> Option<Value> {
    let decision = |decision: &str| to_body(&Decision { decision });
    Some(match (codex, answer) {
        (
            CodexAsk::Command {
                amendment: Some(amendment),
                ..
            },
            AskAnswer::AllowAlways,
        ) => to_body(&Decision {
            decision: AmendedDecision::AcceptWithExecpolicyAmendment(Amendment {
                execpolicy_amendment: amendment,
            }),
        }),
        (CodexAsk::Command { .. } | CodexAsk::FileChange, AskAnswer::AllowAlways) => {
            decision(ACCEPT_FOR_SESSION)
        }
        (CodexAsk::Command { declinable, .. }, AskAnswer::Deny { .. }) => {
            decision(if *declinable { DECLINE } else { CANCEL })
        }
        (CodexAsk::FileChange, AskAnswer::Deny { .. }) => decision(DECLINE),
        (
            CodexAsk::Command { .. } | CodexAsk::FileChange,
            AskAnswer::Allow | AskAnswer::ApprovePlan { .. } | AskAnswer::Answers(_),
        ) => decision(ACCEPT),
        (CodexAsk::UserInput { questions }, AskAnswer::Answers(picked)) => to_body(&Answers {
            answers: questions
                .iter()
                .zip(picked)
                .map(|(id, picked)| {
                    let answered = Answered {
                        answers: picked.labels.clone(),
                    };
                    (id.clone(), to_body(&answered))
                })
                .collect(),
        }),
        (CodexAsk::UserInput { .. }, _) => to_body(&Answers {
            answers: Map::new(),
        }),
        (CodexAsk::Permissions { .. }, AskAnswer::Deny { .. }) => return None,
        (CodexAsk::Permissions { requested }, AskAnswer::AllowAlways) => to_body(&Granted {
            permissions: requested,
            scope: SESSION_SCOPE,
        }),
        (CodexAsk::Permissions { requested }, _) => to_body(&Granted {
            permissions: requested,
            scope: TURN_SCOPE,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::PickedAnswer;
    use serde_json::json;

    fn command_ask(params: &Value) -> ToolAsk {
        server_ask(RpcId::Number(0), COMMAND_APPROVAL, params, &HashMap::new())
            .unwrap_or_else(|| unreachable!("a command approval is an ask"))
    }

    fn codex(ask: &ToolAsk) -> &CodexAsk {
        let Responder::Codex { ask, .. } = &ask.responder else {
            unreachable!("a codex ask answers codex");
        };
        ask
    }

    #[test]
    fn a_command_shows_what_runs_and_can_be_allowed_with_its_amendment() {
        let ask = command_ask(&json!({
            "command": "/bin/zsh -lc 'cat notes.txt'",
            "commandActions": [{"type": "read", "command": "cat notes.txt", "name": "notes.txt", "path": "/p/notes.txt"}],
            "proposedExecpolicyAmendment": ["cat", "notes.txt"],
            "availableDecisions": ["accept", {"acceptWithExecpolicyAmendment": {"execpolicy_amendment": ["cat", "notes.txt"]}}, "cancel"]
        }));
        assert!(ask.can_allow_always());
        assert_eq!(
            ask.kind,
            AskKind::Permission {
                tool: "Bash".to_owned(),
                kind: ToolKind::Shell,
                summary: "cat notes.txt".to_owned(),
                title: None,
                edit: None,
            }
        );
        assert_eq!(
            reply(codex(&ask), &AskAnswer::AllowAlways),
            Some(
                json!({"decision": {"acceptWithExecpolicyAmendment": {"execpolicy_amendment": ["cat", "notes.txt"]}}})
            )
        );
        let denied = AskAnswer::Deny {
            message: String::new(),
        };
        assert_eq!(
            reply(codex(&ask), &denied),
            Some(json!({"decision": "cancel"}))
        );
        assert_eq!(
            reply(codex(&ask), &AskAnswer::Allow),
            Some(json!({"decision": "accept"}))
        );
    }

    #[test]
    fn a_file_change_shows_the_edit_it_waits_on() {
        let tool = ToolCall {
            id: None,
            name: "Write".to_owned(),
            kind: ToolKind::Edit,
            summary: "out.txt".to_owned(),
            edit: None,
        };
        let changes = HashMap::from([("item_1".to_owned(), vec![tool])]);
        let ask = server_ask(
            RpcId::Number(4),
            FILE_APPROVAL,
            &json!({"itemId": "item_1", "threadId": "t", "turnId": "u", "startedAtMs": 1}),
            &changes,
        )
        .unwrap_or_else(|| unreachable!("a file approval is an ask"));
        assert!(
            matches!(&ask.kind, AskKind::Permission { tool, summary, .. } if tool == "Write" && summary == "out.txt")
        );
        assert_eq!(
            reply(codex(&ask), &AskAnswer::AllowAlways),
            Some(json!({"decision": "acceptForSession"}))
        );
    }

    #[test]
    fn answers_go_back_under_the_ids_of_their_questions() {
        let ask = server_ask(
            RpcId::Text("q".to_owned()),
            USER_INPUT,
            &json!({"questions": [{"id": "db", "header": "Database", "question": "Which one?",
                "options": [{"label": "Postgres", "description": "server"}, {"label": "SQLite", "description": "file"}]}]}),
            &HashMap::new(),
        )
        .unwrap_or_else(|| unreachable!("a question is an ask"));
        let answers = AskAnswer::Answers(vec![PickedAnswer {
            question: "Which one?".to_owned(),
            labels: vec!["SQLite".to_owned()],
        }]);
        assert_eq!(
            reply(codex(&ask), &answers),
            Some(json!({"answers": {"db": {"answers": ["SQLite"]}}}))
        );
    }
}
