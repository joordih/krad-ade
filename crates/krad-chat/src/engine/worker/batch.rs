use krad_domain::{ChatEvent, Checklist, PermissionMode, ToolCall, ToolUseId};
use std::sync::Arc;
use std::thread;

use super::subagents::Subagents;
use super::{Inbound, Phase, Worker};
use crate::control::{AskKind, LiveEvent, Question, Responder, ToolAsk};
use crate::engine::call::EngineCall;
use crate::engine::checklist;
use crate::engine::plan;
use crate::engine::policy::{verdict, Verdict};
use crate::engine::tools::{Action, Context, Ran, SideRequest};
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

fn engine_ask(call: &Call, kind: AskKind) -> ToolAsk {
    ToolAsk {
        responder: Responder::Engine {
            call: call.id.clone(),
        },
        kind,
    }
}

fn permission_ask(row: &ToolCall) -> AskKind {
    AskKind::Permission {
        tool: row.name.clone(),
        kind: row.kind,
        summary: row.summary.clone(),
        title: None,
        edit: row.edit.clone(),
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
            let workspace = &self.setup.workspace;
            let parsed = EngineCall::parse(&call);
            let row = parsed.row(&call.id, workspace);
            match parsed {
                EngineCall::Questions(Ok(asked)) => {
                    batch.waiting = Waiting::Questions;
                    return self.ask_questions(&call, row, asked);
                }
                EngineCall::Checklist(Ok(list)) => self.update_checklist(row, list),
                EngineCall::Agent(task) => {
                    if let Some(report) = batch.subagents.take_report(&call.id) {
                        batch.settle(report.content, report.failed);
                        continue;
                    }
                    if let Err(error) = task {
                        batch.settle(error, true);
                        continue;
                    }
                    batch.waiting = Waiting::Subagent;
                    return;
                }
                EngineCall::Plan(Ok(proposed)) => {
                    self.emit(ChatEvent::Tool(row));
                    let Phase::Tools(batch) = &mut self.phase else {
                        return;
                    };
                    if self.mode != PermissionMode::Plan {
                        batch.settle(plan::NOT_IN_PLAN_TEXT.to_owned(), false);
                        continue;
                    }
                    batch.waiting = Waiting::Plan;
                    let ask = engine_ask(&call, AskKind::Plan { plan: proposed });
                    return (self.sink)(LiveEvent::Ask(Box::new(ask)));
                }
                EngineCall::Tool(action) => {
                    if self.use_tool(&call, row, action) {
                        return;
                    }
                }
                EngineCall::Questions(Err(error))
                | EngineCall::Checklist(Err(error))
                | EngineCall::Plan(Err(error))
                | EngineCall::Unparsed { error, .. } => batch.settle(error, true),
            }
        }
    }

    fn use_tool(&mut self, call: &Call, row: ToolCall, action: Action) -> bool {
        let workspace = &self.setup.workspace;
        let row = ToolCall {
            edit: action.preview(workspace),
            ..row
        };
        let decided = verdict(&action, self.mode, workspace, &self.allowed);
        let Phase::Tools(batch) = &mut self.phase else {
            return true;
        };
        match decided {
            Verdict::Deny(reason) => {
                batch.settle(reason.to_owned(), true);
                self.emit(ChatEvent::Tool(row));
                false
            }
            Verdict::Allow => {
                batch.waiting = Waiting::Run;
                self.emit(ChatEvent::Tool(row));
                self.spawn_run(action);
                true
            }
            Verdict::Ask => {
                let ask = engine_ask(call, permission_ask(&row));
                batch.waiting = Waiting::Answer(action);
                self.emit(ChatEvent::Tool(row));
                (self.sink)(LiveEvent::Ask(Box::new(ask)));
                true
            }
        }
    }

    fn update_checklist(&mut self, row: ToolCall, list: Checklist) {
        let Phase::Tools(batch) = &mut self.phase else {
            return;
        };
        batch.settle(checklist::recorded(&list), false);
        self.emit(ChatEvent::Tool(row));
        self.emit(ChatEvent::Checklist(list));
    }

    fn ask_questions(&mut self, call: &Call, row: ToolCall, asked: Vec<Question>) {
        self.emit(ChatEvent::Tool(row));
        let ask = engine_ask(call, AskKind::Questions(asked));
        (self.sink)(LiveEvent::Ask(Box::new(ask)));
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
        let side = self.model.as_ref().map(|model| SideRequest {
            access: self.setup.access.clone(),
            model: model.id.clone(),
            vendor: model.vendor,
        });
        Context {
            workspace: self.setup.workspace.clone(),
            background: self.background.clone(),
            side,
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
