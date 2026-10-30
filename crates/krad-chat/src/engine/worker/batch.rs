use krad_domain::{ChatEvent, FileEdit, PermissionMode, ToolCall, ToolUseId};
use serde_json::Value;
use std::sync::Arc;
use std::thread;

use super::subagents::Subagents;
use super::{Inbound, Phase, Worker};
use crate::checklist::TODO_TOOL;
use crate::control::{AskKind, LiveEvent, Question, Responder, ToolAsk};
use crate::engine::agent::{self, AGENT_TOOL};
use crate::engine::checklist::{self, CHECKLIST_TOOL};
use crate::engine::plan::{self, PLAN_LABEL, PLAN_TOOL};
use crate::engine::policy::{verdict, Verdict};
use crate::engine::questions::{self, QUESTIONS_LABEL, QUESTIONS_TOOL};
use crate::engine::tools::{Action, Context, Ran, Search, Workspace};
use crate::engine::wire::{Call, Message, Outcome};

const DENIED_TEXT: &str = "The user denied this tool call.";
const DENIED_REASON_LABEL: &str = "Reason:";
const INTERRUPTED_TEXT: &str = "The user interrupted the turn before this tool call ran.";
const SNAPSHOT_FAILED_LEAD: &str = "Could not keep a copy of the file for a rewind: ";

enum Waiting {
    Nothing,
    Answer(Action),
    Questions,
    Plan,
    Run,
    Subagent,
}

pub(super) struct Batch {
    pub(super) calls: Vec<Call>,
    outcomes: Vec<Outcome>,
    waiting: Waiting,
    pub(super) subagents: Subagents,
}

impl Batch {
    pub(super) fn new(calls: Vec<Call>) -> Self {
        Self {
            calls,
            outcomes: Vec::new(),
            waiting: Waiting::Nothing,
            subagents: Subagents::default(),
        }
    }

    pub(super) const fn waits_on_subagent(&self) -> bool {
        matches!(self.waiting, Waiting::Subagent)
    }

    fn current(&self) -> Option<&Call> {
        self.calls.get(self.outcomes.len())
    }

    pub(super) fn awaits(&self, request_id: &str) -> bool {
        matches!(
            self.waiting,
            Waiting::Answer(_) | Waiting::Questions | Waiting::Plan
        ) && self.current().is_some_and(|call| call.id == request_id)
    }

    pub(super) const fn asks_questions(&self) -> bool {
        matches!(self.waiting, Waiting::Questions)
    }

    pub(super) const fn asks_plan(&self) -> bool {
        matches!(self.waiting, Waiting::Plan)
    }

    pub(super) const fn waiting_action(&self) -> Option<&Action> {
        match &self.waiting {
            Waiting::Answer(action) => Some(action),
            Waiting::Nothing
            | Waiting::Questions
            | Waiting::Plan
            | Waiting::Run
            | Waiting::Subagent => None,
        }
    }

    pub(super) fn settle(&mut self, content: String, failed: bool) {
        let Some(call) = self.current() else {
            return;
        };
        self.outcomes.push(Outcome {
            call: call.id.clone(),
            name: call.name.clone(),
            content,
            failed,
        });
        self.waiting = Waiting::Nothing;
    }

    pub(super) fn deny(&mut self, message: &str) {
        let message = message.trim();
        let content = if message.is_empty() {
            DENIED_TEXT.to_owned()
        } else {
            format!("{DENIED_TEXT} {DENIED_REASON_LABEL} {message}")
        };
        self.settle(content, true);
    }

    pub(super) fn interrupted(mut self) -> Vec<Outcome> {
        while self.current().is_some() {
            self.settle(INTERRUPTED_TEXT.to_owned(), true);
        }
        self.outcomes
    }
}

fn tool_event(call: &Call, action: &Action, workspace: &Workspace) -> ChatEvent {
    ChatEvent::Tool(ToolCall {
        id: Some(ToolUseId::from(call.id.as_str())),
        name: action.label().into_owned(),
        summary: action.summary(workspace),
        edit: action.preview(workspace),
    })
}

fn plan_event(call: &Call, proposed: &str) -> ChatEvent {
    ChatEvent::Tool(ToolCall {
        id: Some(ToolUseId::from(call.id.as_str())),
        name: PLAN_LABEL.to_owned(),
        summary: plan::summary(proposed),
        edit: None,
    })
}

fn plan_ask(call: &Call, proposed: String) -> ToolAsk {
    ToolAsk {
        request_id: call.id.clone(),
        input: call.arguments.clone(),
        suggestions: Vec::new(),
        responder: Responder::Engine,
        kind: AskKind::Plan { plan: proposed },
    }
}

fn permission_ask(
    call: &Call,
    action: &Action,
    edit: Option<FileEdit>,
    workspace: &Workspace,
) -> ToolAsk {
    ToolAsk {
        request_id: call.id.clone(),
        input: call.arguments.clone(),
        suggestions: vec![Value::Bool(true)],
        responder: Responder::Engine,
        kind: AskKind::Permission {
            tool: action.label().into_owned(),
            summary: action.summary(workspace),
            title: None,
            edit,
        },
    }
}

impl<S: FnMut(LiveEvent)> Worker<S> {
    pub(super) fn advance_batch(&mut self) {
        loop {
            let Phase::Tools(batch) = &mut self.phase else {
                return;
            };
            let Some(call) = batch.current().cloned() else {
                return self.finish_batch();
            };
            if call.name == QUESTIONS_TOOL {
                match questions::parse(&call.arguments) {
                    Ok(asked) => {
                        batch.waiting = Waiting::Questions;
                        return self.ask_questions(&call, asked);
                    }
                    Err(error) => {
                        batch.settle(error, true);
                        continue;
                    }
                }
            }
            if call.name == CHECKLIST_TOOL {
                self.update_checklist(&call);
                continue;
            }
            if call.name == AGENT_TOOL {
                if let Some(report) = batch.subagents.take_report(&call.id) {
                    batch.settle(report.content, report.failed);
                    continue;
                }
                if let Err(error) = agent::parse(&call.arguments) {
                    batch.settle(error, true);
                    continue;
                }
                batch.waiting = Waiting::Subagent;
                return;
            }
            if call.name == PLAN_TOOL {
                let proposed = match plan::parse(&call.arguments) {
                    Ok(proposed) => proposed,
                    Err(error) => {
                        batch.settle(error, true);
                        continue;
                    }
                };
                self.emit(plan_event(&call, &proposed));
                let Phase::Tools(batch) = &mut self.phase else {
                    return;
                };
                if self.mode != PermissionMode::Plan {
                    batch.settle(plan::not_in_plan().to_owned(), false);
                    continue;
                }
                batch.waiting = Waiting::Plan;
                return (self.sink)(LiveEvent::Ask(Box::new(plan_ask(&call, proposed))));
            }
            let workspace = &self.setup.workspace;
            let action = match Action::parse(&call.name, &call.arguments) {
                Ok(action) => action,
                Err(error) => {
                    batch.settle(error, true);
                    continue;
                }
            };
            let event = tool_event(&call, &action, workspace);
            let preview = match &event {
                ChatEvent::Tool(tool) => tool.edit.clone(),
                _ => None,
            };
            match verdict(&action, self.mode, workspace, &self.allowed) {
                Verdict::Deny(reason) => {
                    batch.settle(reason.to_owned(), true);
                    self.emit(event);
                }
                Verdict::Allow => {
                    batch.waiting = Waiting::Run;
                    self.emit(event);
                    return self.spawn_run(action);
                }
                Verdict::Ask => {
                    let ask = permission_ask(&call, &action, preview, workspace);
                    batch.waiting = Waiting::Answer(action);
                    self.emit(event);
                    return (self.sink)(LiveEvent::Ask(Box::new(ask)));
                }
            }
        }
    }

    fn update_checklist(&mut self, call: &Call) {
        let Phase::Tools(batch) = &mut self.phase else {
            return;
        };
        let list = match checklist::parse(&call.arguments) {
            Ok(list) => list,
            Err(error) => return batch.settle(error, true),
        };
        batch.settle(checklist::recorded(&list), false);
        self.emit(ChatEvent::Tool(ToolCall {
            id: Some(ToolUseId::from(call.id.as_str())),
            name: TODO_TOOL.to_owned(),
            summary: checklist::summary(&list),
            edit: None,
        }));
        self.emit(ChatEvent::Checklist(list));
    }

    fn ask_questions(&mut self, call: &Call, asked: Vec<Question>) {
        self.emit(ChatEvent::Tool(ToolCall {
            id: Some(ToolUseId::from(call.id.as_str())),
            name: QUESTIONS_LABEL.to_owned(),
            summary: questions::summary(&asked),
            edit: None,
        }));
        (self.sink)(LiveEvent::Ask(Box::new(ToolAsk {
            request_id: call.id.clone(),
            input: call.arguments.clone(),
            suggestions: Vec::new(),
            responder: Responder::Engine,
            kind: AskKind::Questions(asked),
        })));
    }

    pub(super) fn run_waiting(&mut self) {
        let Phase::Tools(batch) = &mut self.phase else {
            return;
        };
        let Waiting::Answer(action) = std::mem::replace(&mut batch.waiting, Waiting::Run) else {
            return;
        };
        self.spawn_run(action);
    }

    fn context(&self) -> Context {
        let search = self.model.as_ref().map(|model| Search {
            access: self.setup.access.clone(),
            model: model.clone(),
            protocol: self.protocol(model),
        });
        Context {
            workspace: self.setup.workspace.clone(),
            background: self.background.clone(),
            search,
            mcp: Arc::clone(&self.setup.mcp),
        }
    }

    fn keep_before_writing(&mut self, action: &Action) {
        let (Some(turn), Some(path)) = (self.turn, action.written(&self.setup.workspace)) else {
            return;
        };
        if let Err(error) = self.snapshots.keep(turn, &path) {
            (self.sink)(LiveEvent::Notice(format!("{SNAPSHOT_FAILED_LEAD}{error}")));
        }
    }

    fn spawn_run(&mut self, action: Action) {
        self.keep_before_writing(&action);
        let (attempt, cancel) = self.next_attempt();
        let context = self.context();
        let inbound = self.inbound.clone();
        thread::spawn(move || {
            let ran = action.run(&context, &cancel);
            inbound
                .send(Inbound::Ran { attempt, ran })
                .unwrap_or_default();
        });
    }

    pub(super) fn ran(&mut self, ran: Ran) {
        let Phase::Tools(batch) = &mut self.phase else {
            return;
        };
        let tool = batch
            .current()
            .map(|call| ToolUseId::from(call.id.as_str()));
        batch.settle(ran.content, ran.failed);
        if let (Some(tool), Some(edit)) = (tool, ran.edit) {
            self.emit(ChatEvent::Edited { tool, edit });
        }
        self.advance_batch();
    }

    fn finish_batch(&mut self) {
        let Phase::Tools(batch) = std::mem::replace(&mut self.phase, Phase::Streaming) else {
            return;
        };
        self.push_message(Message::Results {
            outcomes: batch.outcomes,
        });
        self.deliver_queued();
        self.begin_request();
    }
}
