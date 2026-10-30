use krad_domain::{ChatEvent, FileEdit, ModelId, PermissionMode, RewindId, ToolKind};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use std::borrow::Cow;

use crate::codex::{CodexAsk, RpcId};
use crate::edits::file_edit;
use crate::parse::tool_summary;
use crate::refused::ResumeRefused;
use crate::rewind::{RewindError, Rewound};

mod catalog;
mod held;

pub use catalog::{Catalog, CommandOffer, ModelOffer, ModelSource, SourceRank};

const ANSWERS_KEY: &str = "answers";
const LABEL_SEPARATOR: &str = ", ";

fn each_entry<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let Value::Array(entries) = Value::deserialize(deserializer)? else {
        return Ok(Vec::new());
    };
    Ok(entries
        .into_iter()
        .filter_map(|entry| T::deserialize(entry).ok())
        .collect())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlanUsage {
    pub five_hour: Option<u16>,
    pub seven_day: Option<u16>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LiveEvent {
    Chat(ChatEvent),
    Ask(Box<ToolAsk>),
    Ready(Catalog),
    Usage(PlanUsage),
    Notice(String),
    Rewound(Result<Rewound, RewindError>),
    Refused(ResumeRefused),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    #[serde(rename = "question")]
    pub text: String,
    pub header: Option<String>,
    #[serde(default, deserialize_with = "each_entry")]
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multi_select: bool,
}

#[derive(Default, Deserialize)]
struct PlanInput {
    #[serde(default)]
    plan: String,
}

#[derive(Default, Deserialize)]
struct QuestionInput {
    #[serde(default, deserialize_with = "each_entry")]
    questions: Vec<Question>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AskKind {
    Permission {
        tool: String,
        kind: ToolKind,
        summary: String,
        title: Option<String>,
        edit: Option<FileEdit>,
    },
    Questions(Vec<Question>),
    Plan {
        plan: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FormValue {
    pub(crate) label: String,
    pub(crate) value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FormSlot {
    pub(crate) key: String,
    pub(crate) many: bool,
    pub(crate) values: Vec<FormValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClaudeAsk {
    pub(crate) request_id: String,
    pub(crate) input: Value,
    pub(crate) suggestions: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Responder {
    ClaudeCode(ClaudeAsk),
    Engine {
        call: String,
    },
    OpencodePermission {
        session: String,
        request: String,
        savable: bool,
    },
    OpencodeForm {
        session: String,
        form: String,
        slots: Vec<FormSlot>,
    },
    Codex {
        id: RpcId,
        ask: CodexAsk,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolAsk {
    pub(crate) responder: Responder,
    pub kind: AskKind,
}

impl ToolAsk {
    #[must_use]
    pub fn can_allow_always(&self) -> bool {
        match &self.responder {
            Responder::ClaudeCode(claude) => !claude.suggestions.is_empty(),
            Responder::Engine { .. } => matches!(self.kind, AskKind::Permission { .. }),
            Responder::OpencodePermission { savable, .. } => *savable,
            Responder::OpencodeForm { .. } => false,
            Responder::Codex { ask, .. } => ask.can_allow_always(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickedAnswer {
    pub question: String,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AskAnswer {
    Allow,
    AllowAlways,
    ApprovePlan { mode: PermissionMode },
    Deny { message: String },
    Answers(Vec<PickedAnswer>),
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum IncomingLine {
    ControlRequest {
        request_id: String,
        request: IncomingRequest,
    },
    ControlResponse {
        response: IncomingResponse,
    },
    #[serde(other)]
    Conversation,
}

#[derive(Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
enum IncomingRequest {
    CanUseTool {
        tool_name: String,
        #[serde(default)]
        input: Value,
        title: Option<String>,
        description: Option<String>,
        #[serde(default)]
        permission_suggestions: Vec<Value>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
enum IncomingResponse {
    Success {
        request_id: String,
        #[serde(default)]
        response: Value,
    },
    Error {
        request_id: String,
        #[serde(default)]
        error: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Routed {
    Ask(Box<ToolAsk>),
    Ready(Catalog),
    Replied {
        request_id: String,
        reply: Result<Value, String>,
    },
    Control,
    Conversation(Value),
}

pub(crate) fn route(line: &str, init_id: &str) -> Option<Routed> {
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    let Ok(incoming) = IncomingLine::deserialize(&value) else {
        return Some(Routed::Conversation(value));
    };
    Some(match incoming {
        IncomingLine::ControlRequest {
            request_id,
            request,
        } => {
            tool_ask(request_id, request).map_or(Routed::Control, |ask| Routed::Ask(Box::new(ask)))
        }
        IncomingLine::ControlResponse {
            response:
                IncomingResponse::Success {
                    request_id,
                    response,
                },
        } if request_id == init_id => {
            Routed::Ready(Catalog::deserialize(&response).unwrap_or_default())
        }
        IncomingLine::ControlResponse {
            response:
                IncomingResponse::Success {
                    request_id,
                    response,
                },
        } => Routed::Replied {
            request_id,
            reply: Ok(response),
        },
        IncomingLine::ControlResponse {
            response: IncomingResponse::Error { request_id, error },
        } => Routed::Replied {
            request_id,
            reply: Err(error),
        },
        IncomingLine::ControlResponse {
            response: IncomingResponse::Other,
        } => Routed::Control,
        IncomingLine::Conversation => Routed::Conversation(value),
    })
}

fn tool_ask(request_id: String, request: IncomingRequest) -> Option<ToolAsk> {
    let IncomingRequest::CanUseTool {
        tool_name,
        input,
        title,
        description,
        permission_suggestions,
    } = request
    else {
        return None;
    };
    let title = title.or(description);
    Some(ask_of(
        request_id,
        tool_name,
        input,
        title,
        permission_suggestions,
    ))
}

fn ask_of(
    request_id: String,
    tool_name: String,
    input: Value,
    title: Option<String>,
    suggestions: Vec<Value>,
) -> ToolAsk {
    let kind = match ToolKind::of_claude_code(&tool_name) {
        ToolKind::Question => {
            let asked = QuestionInput::deserialize(&input).unwrap_or_default();
            AskKind::Questions(asked.questions)
        }
        ToolKind::Plan => {
            let asked = PlanInput::deserialize(&input).unwrap_or_default();
            AskKind::Plan { plan: asked.plan }
        }
        kind => AskKind::Permission {
            summary: tool_summary(Some(&input)),
            edit: file_edit(&tool_name, Some(&input)),
            tool: tool_name,
            kind,
            title,
        },
    };
    ToolAsk {
        responder: Responder::ClaudeCode(ClaudeAsk {
            request_id,
            input,
            suggestions,
        }),
        kind,
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutgoingLine<'a> {
    ControlRequest {
        request_id: &'a str,
        request: OutgoingRequest<'a>,
    },
    ControlResponse {
        response: OutgoingResponse<'a>,
    },
}

#[derive(Serialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
enum OutgoingRequest<'a> {
    Initialize,
    Interrupt,
    SetModel { model: &'a ModelId },
    SetPermissionMode { mode: PermissionMode },
    RewindFiles { user_message_id: &'a str },
}

#[derive(Serialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
enum OutgoingResponse<'a> {
    Success {
        request_id: &'a str,
        response: Decision<'a>,
    },
}

#[derive(Serialize)]
#[serde(tag = "behavior", rename_all = "snake_case")]
enum Decision<'a> {
    Allow {
        #[serde(rename = "updatedInput")]
        updated_input: Cow<'a, Value>,
        #[serde(rename = "updatedPermissions", skip_serializing_if = "Vec::is_empty")]
        updated_permissions: Vec<PermissionUpdate<'a>>,
    },
    Deny {
        message: &'a str,
    },
}

#[derive(Serialize)]
#[serde(untagged)]
enum PermissionUpdate<'a> {
    Suggested(&'a Value),
    Mode(ModeUpdate),
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ModeUpdate {
    SetMode {
        mode: PermissionMode,
        destination: Destination,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum Destination {
    Session,
}

pub(crate) fn json_line(message: &impl Serialize) -> String {
    let mut line = serde_json::to_string(message)
        .unwrap_or_else(|_| unreachable!("protocol lines only have string keys"));
    line.push('\n');
    line
}

fn control_request(request_id: &str, request: OutgoingRequest<'_>) -> String {
    json_line(&OutgoingLine::ControlRequest {
        request_id,
        request,
    })
}

pub(crate) fn initialize_line(request_id: &str) -> String {
    control_request(request_id, OutgoingRequest::Initialize)
}

pub(crate) fn interrupt_line(request_id: &str) -> String {
    control_request(request_id, OutgoingRequest::Interrupt)
}

pub(crate) fn set_model_line(request_id: &str, model: &ModelId) -> String {
    control_request(request_id, OutgoingRequest::SetModel { model })
}

pub(crate) fn set_permission_mode_line(request_id: &str, mode: PermissionMode) -> String {
    control_request(request_id, OutgoingRequest::SetPermissionMode { mode })
}

pub(crate) fn rewind_files_line(request_id: &str, prompt: &RewindId) -> String {
    control_request(
        request_id,
        OutgoingRequest::RewindFiles {
            user_message_id: prompt.as_str(),
        },
    )
}

fn answered_input(input: &Value, answers: &[PickedAnswer]) -> Value {
    let mut input = input.clone();
    let answers: Map<String, Value> = answers
        .iter()
        .map(|picked| {
            let labels = picked.labels.join(LABEL_SEPARATOR);
            (picked.question.clone(), Value::from(labels))
        })
        .collect();
    if let Some(object) = input.as_object_mut() {
        object.insert(ANSWERS_KEY.to_owned(), Value::Object(answers));
    }
    input
}

pub(crate) fn answer_line(ask: &ClaudeAsk, answer: &AskAnswer) -> String {
    let allow = |updated_permissions| Decision::Allow {
        updated_input: Cow::Borrowed(&ask.input),
        updated_permissions,
    };
    let response = match answer {
        AskAnswer::Allow => allow(Vec::new()),
        AskAnswer::AllowAlways => allow(
            ask.suggestions
                .iter()
                .map(PermissionUpdate::Suggested)
                .collect(),
        ),
        AskAnswer::ApprovePlan { mode } => {
            allow(vec![PermissionUpdate::Mode(ModeUpdate::SetMode {
                mode: *mode,
                destination: Destination::Session,
            })])
        }
        AskAnswer::Deny { message } => Decision::Deny { message },
        AskAnswer::Answers(answers) => Decision::Allow {
            updated_input: Cow::Owned(answered_input(&ask.input, answers)),
            updated_permissions: Vec::new(),
        },
    };
    json_line(&OutgoingLine::ControlResponse {
        response: OutgoingResponse::Success {
            request_id: &ask.request_id,
            response,
        },
    })
}

#[cfg(test)]
mod tests;
