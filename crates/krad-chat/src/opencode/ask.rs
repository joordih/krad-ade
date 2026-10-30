use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::chat::session_path;
use super::events::PermissionAsked;
use super::tools::harness_tool;
use crate::control::{
    AskAnswer, AskKind, FormSlot, FormValue, PickedAnswer, Question, QuestionOption, Responder,
    ToolAsk,
};

const RESOURCE_SEPARATOR: &str = ", ";
const MULTISELECT: &str = "multiselect";

#[derive(Debug, Deserialize)]
pub(crate) struct FormOption {
    value: String,
    label: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FormField {
    key: String,
    title: Option<String>,
    description: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    options: Vec<FormOption>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FormInfo {
    id: String,
    #[serde(rename = "sessionID")]
    pub(crate) session_id: String,
    #[serde(default)]
    fields: Vec<FormField>,
}

pub(crate) fn permission_ask(asked: &PermissionAsked) -> ToolAsk {
    let (tool, kind) = harness_tool(&asked.action);
    ToolAsk {
        responder: Responder::OpencodePermission {
            session: asked.session_id.clone(),
            request: asked.id.clone(),
            savable: !asked.save.is_empty(),
        },
        kind: AskKind::Permission {
            tool: tool.to_owned(),
            kind,
            summary: asked.resources.join(RESOURCE_SEPARATOR),
            title: asked.message.clone(),
            edit: None,
        },
    }
}

pub(crate) fn form_ask(form: &FormInfo) -> Option<ToolAsk> {
    if form.fields.is_empty() {
        return None;
    }
    let questions = form
        .fields
        .iter()
        .map(|field| Question {
            text: field
                .title
                .clone()
                .or_else(|| field.description.clone())
                .unwrap_or_else(|| field.key.clone()),
            header: field.title.as_ref().and(field.description.clone()),
            options: field
                .options
                .iter()
                .map(|option| QuestionOption {
                    label: option.label.clone(),
                    description: option.description.clone(),
                })
                .collect(),
            multi_select: field.kind == MULTISELECT,
        })
        .collect();
    let slots = form
        .fields
        .iter()
        .map(|field| FormSlot {
            key: field.key.clone(),
            many: field.kind == MULTISELECT,
            values: field
                .options
                .iter()
                .map(|option| FormValue {
                    label: option.label.clone(),
                    value: option.value.clone(),
                })
                .collect(),
        })
        .collect();
    Some(ToolAsk {
        responder: Responder::OpencodeForm {
            session: form.session_id.clone(),
            form: form.id.clone(),
            slots,
        },
        kind: AskKind::Questions(questions),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Decision {
    Once,
    Always,
    Reject,
}

#[derive(Serialize)]
struct PermissionReply<'a> {
    decision: Decision,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
}

#[derive(Serialize)]
struct FormReply {
    answer: Map<String, Value>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Reply {
    Post { path: String, body: Value },
    Delete { path: String },
}

fn permission_path(session: &str, request: &str) -> String {
    session_path(session, &format!("/permission/{request}/reply"))
}

fn form_path(session: &str, form: &str) -> String {
    session_path(session, &format!("/form/{form}"))
}

fn picked_value(slot: &FormSlot, label: &str) -> Value {
    slot.values
        .iter()
        .find(|offered| offered.label == label)
        .map_or_else(
            || Value::from(label),
            |offered| Value::from(offered.value.clone()),
        )
}

fn form_answer(slots: &[FormSlot], answers: &[PickedAnswer]) -> Map<String, Value> {
    slots
        .iter()
        .zip(answers)
        .map(|(slot, picked)| {
            let mut values = picked.labels.iter().map(|label| picked_value(slot, label));
            let value = if slot.many {
                Value::Array(values.collect())
            } else {
                values.next().unwrap_or_default()
            };
            (slot.key.clone(), value)
        })
        .collect()
}

fn to_body(body: &impl Serialize) -> Value {
    serde_json::to_value(body).unwrap_or_default()
}

pub(crate) fn reply(ask: &ToolAsk, answer: &AskAnswer) -> Option<Reply> {
    match &ask.responder {
        Responder::ClaudeCode(_) | Responder::Engine { .. } | Responder::Codex { .. } => None,
        Responder::OpencodePermission {
            session, request, ..
        } => {
            let (decision, message) = match answer {
                AskAnswer::AllowAlways => (Decision::Always, None),
                AskAnswer::Deny { message } => (
                    Decision::Reject,
                    Some(message.as_str()).filter(|message| !message.is_empty()),
                ),
                AskAnswer::Allow | AskAnswer::ApprovePlan { .. } | AskAnswer::Answers(_) => {
                    (Decision::Once, None)
                }
            };
            Some(Reply::Post {
                path: permission_path(session, request),
                body: to_body(&PermissionReply { decision, message }),
            })
        }
        Responder::OpencodeForm {
            session,
            form,
            slots,
        } => Some(match answer {
            AskAnswer::Answers(answers) => Reply::Post {
                path: format!("{}/reply", form_path(session, form)),
                body: to_body(&FormReply {
                    answer: form_answer(slots, answers),
                }),
            },
            AskAnswer::Allow
            | AskAnswer::AllowAlways
            | AskAnswer::ApprovePlan { .. }
            | AskAnswer::Deny { .. } => Reply::Delete {
                path: form_path(session, form),
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn asked() -> PermissionAsked {
        serde_json::from_value(json!({
            "id": "per_1",
            "sessionID": "ses_a",
            "action": "shell",
            "resources": ["git push origin main"],
            "save": ["git push *"]
        }))
        .unwrap_or_else(|_| unreachable!("the fixture is valid"))
    }

    fn form() -> FormInfo {
        serde_json::from_value(json!({
            "id": "frm_1",
            "sessionID": "ses_a",
            "title": "Questions",
            "fields": [
                {"key": "q0", "type": "string", "title": "Which database?",
                 "options": [{"value": "pg", "label": "Postgres"}, {"value": "lite", "label": "SQLite"}]},
                {"key": "q1", "type": "multiselect", "title": "Which checks?",
                 "options": [{"value": "lint", "label": "Lint"}, {"value": "test", "label": "Tests"}]}
            ]
        }))
        .unwrap_or_else(|_| unreachable!("the fixture is valid"))
    }

    #[test]
    fn a_permission_shows_its_command_and_can_be_allowed_for_good() {
        let ask = permission_ask(&asked());
        assert!(ask.can_allow_always());
        assert_eq!(
            ask.kind,
            AskKind::Permission {
                tool: "Bash".to_owned(),
                kind: krad_domain::ToolKind::Shell,
                summary: "git push origin main".to_owned(),
                title: None,
                edit: None,
            }
        );
        assert_eq!(
            reply(&ask, &AskAnswer::AllowAlways),
            Some(Reply::Post {
                path: "/api/session/ses_a/permission/per_1/reply".to_owned(),
                body: json!({"decision": "always"}),
            })
        );
    }

    #[test]
    fn a_denial_with_a_reason_lets_the_model_carry_on() {
        let ask = permission_ask(&asked());
        let denied = AskAnswer::Deny {
            message: "use a branch".to_owned(),
        };
        assert_eq!(
            reply(&ask, &denied),
            Some(Reply::Post {
                path: "/api/session/ses_a/permission/per_1/reply".to_owned(),
                body: json!({"decision": "reject", "message": "use a branch"}),
            })
        );
    }

    #[test]
    fn answers_go_back_as_the_values_of_the_options_picked() {
        let ask = form_ask(&form()).unwrap_or_else(|| unreachable!("the fixture has it"));
        let AskKind::Questions(questions) = &ask.kind else {
            unreachable!("a form asks questions");
        };
        assert_eq!(questions[0].text, "Which database?");
        assert!(questions[1].multi_select);
        let answers = AskAnswer::Answers(vec![
            PickedAnswer {
                question: "Which database?".to_owned(),
                labels: vec!["SQLite".to_owned()],
            },
            PickedAnswer {
                question: "Which checks?".to_owned(),
                labels: vec!["Lint".to_owned(), "Tests".to_owned()],
            },
        ]);
        assert_eq!(
            reply(&ask, &answers),
            Some(Reply::Post {
                path: "/api/session/ses_a/form/frm_1/reply".to_owned(),
                body: json!({"answer": {"q0": "lite", "q1": ["lint", "test"]}}),
            })
        );
    }

    #[test]
    fn dismissing_the_questions_cancels_the_form() {
        let ask = form_ask(&form()).unwrap_or_else(|| unreachable!("the fixture has it"));
        let denied = AskAnswer::Deny {
            message: String::new(),
        };
        assert_eq!(
            reply(&ask, &denied),
            Some(Reply::Delete {
                path: "/api/session/ses_a/form/frm_1".to_owned(),
            })
        );
    }
}
