use krad_domain::{line_diff, ChatEvent, FileEdit, PermissionMode, ToolCall, ToolUseId};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

use super::ask::{form_ask, permission_ask, FormInfo};
use super::tools::{harness_tool_name, PLAN_AGENT};
use crate::control::LiveEvent;
use crate::parse::tool_summary;

const EDIT_TOOL: &str = "edit";
const WRITE_TOOL: &str = "write";
const PATH_KEY: &str = "path";
const OLD_KEY: &str = "oldString";
const NEW_KEY: &str = "newString";
const CONTENT_KEY: &str = "content";
const UNKNOWN_FAILURE: &str = "opencode reported an error";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Session {
    #[serde(rename = "sessionID")]
    pub(crate) session_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Created {
    #[serde(rename = "sessionID")]
    session_id: String,
    #[serde(rename = "parentID")]
    parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Ended {
    #[serde(rename = "sessionID")]
    session_id: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ToolStarted {
    #[serde(rename = "sessionID")]
    session_id: String,
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ToolCalled {
    #[serde(rename = "sessionID")]
    session_id: String,
    id: String,
    #[serde(default)]
    input: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct ModelRef {
    #[serde(rename = "providerID")]
    pub(crate) provider_id: String,
    pub(crate) id: String,
}

impl ModelRef {
    pub(crate) fn selector(&self) -> String {
        format!("{}/{}", self.provider_id, self.id)
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct StepStarted {
    #[serde(rename = "sessionID")]
    session_id: String,
    model: Option<ModelRef>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub(crate) struct Cache {
    #[serde(default)]
    read: u64,
    #[serde(default)]
    write: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub(crate) struct Tokens {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    cache: Cache,
}

impl Tokens {
    const fn context(self) -> u64 {
        self.input + self.cache.read + self.cache.write
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct StepEnded {
    #[serde(rename = "sessionID")]
    session_id: String,
    tokens: Option<Tokens>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Failure {
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Failed {
    #[serde(rename = "sessionID")]
    session_id: String,
    #[serde(default)]
    error: Failure,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AgentSelected {
    #[serde(rename = "sessionID")]
    session_id: String,
    agent: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PermissionAsked {
    pub(crate) id: String,
    #[serde(rename = "sessionID")]
    pub(crate) session_id: String,
    pub(crate) action: String,
    #[serde(default)]
    pub(crate) resources: Vec<String>,
    #[serde(default)]
    pub(crate) save: Vec<String>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FormCreated {
    pub(crate) form: FormInfo,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
pub(crate) enum ServerEvent {
    #[serde(rename = "session.created")]
    Created(Created),
    #[serde(rename = "session.text.ended")]
    Text(Ended),
    #[serde(rename = "session.reasoning.ended")]
    Reasoning(Ended),
    #[serde(rename = "session.tool.input.started")]
    ToolStarted(ToolStarted),
    #[serde(rename = "session.tool.called")]
    ToolCalled(ToolCalled),
    #[serde(rename = "session.step.started")]
    StepStarted(StepStarted),
    #[serde(rename = "session.step.ended")]
    StepEnded(StepEnded),
    #[serde(rename = "session.execution.started")]
    Started(Session),
    #[serde(rename = "session.execution.succeeded")]
    Succeeded(Session),
    #[serde(rename = "session.execution.interrupted")]
    Interrupted(Session),
    #[serde(rename = "session.execution.failed")]
    Failed(Failed),
    #[serde(rename = "session.idle")]
    Idle(Session),
    #[serde(rename = "session.agent.selected")]
    AgentSelected(AgentSelected),
    #[serde(rename = "permission.asked")]
    PermissionAsked(PermissionAsked),
    #[serde(rename = "form.created")]
    FormCreated(FormCreated),
}

impl ServerEvent {
    pub(crate) fn session(&self) -> &str {
        match self {
            Self::Created(created) => &created.session_id,
            Self::Text(ended) | Self::Reasoning(ended) => &ended.session_id,
            Self::ToolStarted(started) => &started.session_id,
            Self::ToolCalled(called) => &called.session_id,
            Self::StepStarted(step) => &step.session_id,
            Self::StepEnded(step) => &step.session_id,
            Self::Started(session)
            | Self::Succeeded(session)
            | Self::Interrupted(session)
            | Self::Idle(session) => &session.session_id,
            Self::Failed(failed) => &failed.session_id,
            Self::AgentSelected(selected) => &selected.session_id,
            Self::PermissionAsked(asked) => &asked.session_id,
            Self::FormCreated(created) => &created.form.session_id,
        }
    }

    pub(crate) fn parent(&self) -> Option<(&str, &str)> {
        match self {
            Self::Created(Created {
                session_id,
                parent_id: Some(parent),
            }) => Some((session_id, parent)),
            _ => None,
        }
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub(crate) fn opencode_edit(tool: &str, input: &Value) -> Option<FileEdit> {
    let path = input.get(PATH_KEY).and_then(Value::as_str)?.to_owned();
    let lines = match tool {
        EDIT_TOOL => line_diff(text(input, OLD_KEY), text(input, NEW_KEY)),
        WRITE_TOOL => line_diff("", text(input, CONTENT_KEY)),
        _ => return None,
    };
    (!lines.is_empty()).then_some(FileEdit { path, lines })
}

pub(crate) fn tool_call(id: &str, tool: &str, input: &Value) -> ToolCall {
    ToolCall {
        id: Some(ToolUseId::from(id)),
        name: harness_tool_name(tool).to_owned(),
        summary: tool_summary(Some(input)),
        edit: opencode_edit(tool, input),
    }
}

#[derive(Debug, Default)]
pub(crate) struct Translator {
    tools: HashMap<String, String>,
    windows: HashMap<String, u64>,
    pub(crate) mode: Option<PermissionMode>,
}

impl Translator {
    pub(crate) fn new(windows: HashMap<String, u64>, mode: Option<PermissionMode>) -> Self {
        Self {
            tools: HashMap::new(),
            windows,
            mode,
        }
    }

    pub(crate) fn translate(&mut self, event: &ServerEvent, nested: bool) -> Vec<LiveEvent> {
        match event {
            ServerEvent::PermissionAsked(asked) => vec![LiveEvent::Ask(permission_ask(asked))],
            ServerEvent::FormCreated(created) => form_ask(&created.form)
                .map(LiveEvent::Ask)
                .into_iter()
                .collect(),
            _ if nested => Vec::new(),
            ServerEvent::Text(ended) if !ended.text.trim().is_empty() => {
                vec![LiveEvent::Chat(ChatEvent::Text(ended.text.clone()))]
            }
            ServerEvent::Reasoning(ended) if !ended.text.trim().is_empty() => {
                vec![LiveEvent::Chat(ChatEvent::Thinking(
                    ended.text.trim().to_owned(),
                ))]
            }
            ServerEvent::ToolStarted(started) => {
                self.tools.insert(started.id.clone(), started.name.clone());
                Vec::new()
            }
            ServerEvent::ToolCalled(called) => {
                let tool = self.tools.get(&called.id).map_or("", String::as_str);
                vec![LiveEvent::Chat(ChatEvent::Tool(tool_call(
                    &called.id,
                    tool,
                    &called.input,
                )))]
            }
            ServerEvent::StepStarted(StepStarted {
                model: Some(model), ..
            }) => {
                let selector = model.selector();
                let window = self.windows.get(&selector).copied();
                std::iter::once(ChatEvent::Model(selector))
                    .chain(window.map(ChatEvent::ContextWindow))
                    .map(LiveEvent::Chat)
                    .collect()
            }
            ServerEvent::StepEnded(StepEnded {
                tokens: Some(tokens),
                ..
            }) => vec![LiveEvent::Chat(ChatEvent::ContextTokens(tokens.context()))],
            ServerEvent::Succeeded(_) | ServerEvent::Interrupted(_) => {
                vec![LiveEvent::Chat(ChatEvent::Done)]
            }
            ServerEvent::Failed(failed) => vec![LiveEvent::Chat(ChatEvent::Error(
                failed
                    .error
                    .message
                    .clone()
                    .unwrap_or_else(|| UNKNOWN_FAILURE.to_owned()),
            ))],
            ServerEvent::AgentSelected(selected) => {
                let planning = selected.agent == PLAN_AGENT;
                let mode = match self.mode {
                    _ if planning => PermissionMode::Plan,
                    Some(PermissionMode::Plan) | None => PermissionMode::Manual,
                    Some(mode) => mode,
                };
                self.mode = Some(mode);
                vec![LiveEvent::Chat(ChatEvent::PermissionMode(mode))]
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::LineChange;

    fn event(line: &str) -> ServerEvent {
        serde_json::from_str(line).unwrap_or_else(|_| unreachable!("the fixture is valid"))
    }

    fn chat(events: Vec<LiveEvent>) -> Vec<ChatEvent> {
        events
            .into_iter()
            .filter_map(|event| match event {
                LiveEvent::Chat(event) => Some(event),
                LiveEvent::Ask(_) | LiveEvent::Ready(_) => None,
            })
            .collect()
    }

    #[test]
    fn a_turn_becomes_text_tools_usage_and_its_end() {
        let windows = HashMap::from([("anthropic/claude-sonnet-5-5".to_owned(), 200_000)]);
        let mut translator = Translator::new(windows, None);
        let lines = [
            r#"{"type":"session.step.started","data":{"sessionID":"ses_a","assistantMessageID":"msg_1","model":{"providerID":"anthropic","id":"claude-sonnet-5-5"}}}"#,
            r#"{"type":"session.reasoning.ended","data":{"sessionID":"ses_a","assistantMessageID":"msg_1","ordinal":0,"text":"  look first  "}}"#,
            r#"{"type":"session.tool.input.started","data":{"sessionID":"ses_a","assistantMessageID":"msg_1","id":"call_1","name":"shell"}}"#,
            r#"{"type":"session.tool.called","data":{"sessionID":"ses_a","assistantMessageID":"msg_1","id":"call_1","input":{"command":"git status"},"executed":false}}"#,
            r#"{"type":"session.text.ended","data":{"sessionID":"ses_a","assistantMessageID":"msg_1","ordinal":1,"text":"Clean tree."}}"#,
            r#"{"type":"session.step.ended","data":{"sessionID":"ses_a","assistantMessageID":"msg_1","finish":"stop","cost":0.01,"tokens":{"input":1200,"output":85,"reasoning":0,"cache":{"read":9000,"write":10}}}}"#,
            r#"{"type":"session.execution.succeeded","data":{"sessionID":"ses_a"}}"#,
        ];
        let events: Vec<ChatEvent> = lines
            .iter()
            .flat_map(|line| chat(translator.translate(&event(line), false)))
            .collect();
        assert_eq!(
            events,
            [
                ChatEvent::Model("anthropic/claude-sonnet-5-5".to_owned()),
                ChatEvent::ContextWindow(200_000),
                ChatEvent::Thinking("look first".to_owned()),
                ChatEvent::Tool(ToolCall {
                    id: Some(ToolUseId::from("call_1")),
                    name: "Bash".to_owned(),
                    summary: "git status".to_owned(),
                    edit: None,
                }),
                ChatEvent::Text("Clean tree.".to_owned()),
                ChatEvent::ContextTokens(10_210),
                ChatEvent::Done,
            ]
        );
    }

    #[test]
    fn a_failed_execution_carries_the_reason_opencode_gave() {
        let mut translator = Translator::default();
        let failed = event(
            r#"{"type":"session.execution.failed","data":{"sessionID":"ses_a","error":{"type":"provider.free-limit","message":"Free usage limit reached.","status":429}}}"#,
        );
        assert_eq!(
            chat(translator.translate(&failed, false)),
            [ChatEvent::Error("Free usage limit reached.".to_owned())]
        );
    }

    #[test]
    fn a_subagent_only_reaches_the_chat_through_its_questions() {
        let mut translator = Translator::default();
        let text = event(
            r#"{"type":"session.text.ended","data":{"sessionID":"ses_child","text":"inner"}}"#,
        );
        let asked = event(
            r#"{"type":"permission.asked","data":{"id":"per_1","sessionID":"ses_child","action":"shell","resources":["rm -rf build"]}}"#,
        );
        assert_eq!(translator.translate(&text, true), []);
        assert!(matches!(
            translator.translate(&asked, true).as_slice(),
            [LiveEvent::Ask(_)]
        ));
    }

    #[test]
    fn an_edit_shows_its_diff() {
        let edit = opencode_edit(
            EDIT_TOOL,
            &serde_json::json!({"path":"src/a.rs","oldString":"one\n","newString":"two\n"}),
        )
        .unwrap_or_else(|| unreachable!("the fixture has it"));
        assert_eq!(edit.path, "src/a.rs");
        let changes: Vec<LineChange> = edit.lines.iter().map(|line| line.change).collect();
        assert_eq!(changes, [LineChange::Removed, LineChange::Added]);
    }

    #[test]
    fn switching_to_the_plan_agent_reports_plan_mode() {
        let mut translator = Translator::new(HashMap::new(), Some(PermissionMode::AcceptEdits));
        let plan = event(
            r#"{"type":"session.agent.selected","data":{"sessionID":"ses_a","agent":"plan"}}"#,
        );
        let build = event(
            r#"{"type":"session.agent.selected","data":{"sessionID":"ses_a","agent":"build"}}"#,
        );
        assert_eq!(
            chat(translator.translate(&plan, false)),
            [ChatEvent::PermissionMode(PermissionMode::Plan)]
        );
        assert_eq!(
            chat(translator.translate(&build, false)),
            [ChatEvent::PermissionMode(PermissionMode::Manual)]
        );
    }
}
