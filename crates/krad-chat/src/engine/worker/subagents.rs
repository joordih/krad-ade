use krad_domain::{ChatEvent, PermissionMode, Prompt, ToolCall, ToolUseId};
use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use super::compact::text_of;
use super::{Inbound, Order, Phase, Setup, Start, Worker};
use crate::control::{AskAnswer, LiveEvent, ToolAsk};
use crate::engine::agent::{self, Task, AGENT_LABEL, AGENT_TOOL, MAX_SUBAGENTS};
use crate::engine::policy::Allowed;
use crate::engine::session::SessionFile;
use crate::engine::subagents::{subagent_files, write_meta};
use crate::engine::tools::Ran;
use crate::engine::wire::Message;
use crate::live::new_session_id;
use crate::subagents::SubagentMeta;

const ASK_ID_SEPARATOR: char = '/';

pub(crate) enum SubagentNews {
    Asked(Box<ToolAsk>),
    Finished(Ran),
}

struct RoutedAsk {
    call: String,
    inner: String,
}

#[derive(Default)]
pub(super) struct Subagents {
    queued: VecDeque<(String, Task)>,
    running: HashMap<String, Sender<Inbound>>,
    reports: HashMap<String, Ran>,
    asks: HashMap<String, RoutedAsk>,
}

impl Subagents {
    pub(super) fn take_report(&mut self, call: &str) -> Option<Ran> {
        self.reports.remove(call)
    }

    pub(super) fn route(&mut self, ask: &ToolAsk, answer: &AskAnswer) -> bool {
        let Some(routed) = self.asks.remove(&ask.request_id) else {
            return false;
        };
        if let Some(orders) = self.running.get(&routed.call) {
            let inner = ToolAsk {
                request_id: routed.inner,
                ..ask.clone()
            };
            orders
                .send(Inbound::Order(Order::Answer(
                    Box::new(inner),
                    answer.clone(),
                )))
                .unwrap_or_default();
        }
        true
    }

    fn tell(&self, order: impl Fn() -> Order) {
        for orders in self.running.values() {
            orders.send(Inbound::Order(order())).unwrap_or_default();
        }
    }
}

impl Drop for Subagents {
    fn drop(&mut self) {
        self.tell(|| Order::Interrupt);
    }
}

fn meta(call: &str, task: &Task, finished: bool) -> SubagentMeta {
    SubagentMeta {
        tool_use_id: ToolUseId::from(call),
        agent_type: None,
        description: Some(task.description.clone()),
        finished,
    }
}

fn run_subagent(
    start: Start,
    allowed: Allowed,
    own: Sender<Inbound>,
    inbox: &Receiver<Inbound>,
    parent: Sender<Inbound>,
    call: String,
) -> Ran {
    let (failures, failed) = channel();
    let sink = move |event: LiveEvent| match event {
        LiveEvent::Ask(ask) => parent
            .send(Inbound::Subagent {
                call: call.clone(),
                news: SubagentNews::Asked(ask),
            })
            .unwrap_or_default(),
        LiveEvent::Chat(ChatEvent::Error(error)) => failures.send(error).unwrap_or_default(),
        LiveEvent::Chat(_)
        | LiveEvent::Ready(_)
        | LiveEvent::Usage(_)
        | LiveEvent::Notice(_)
        | LiveEvent::Rewound(_) => {}
    };
    let mut worker = Worker::subagent(start, sink, own, allowed);
    worker.run(inbox);
    let (content, failed) = agent::report(worker.last_answer(), failed.try_iter().last());
    Ran {
        content,
        failed,
        edit: None,
    }
}

impl<S: FnMut(LiveEvent)> Worker<S> {
    pub(super) fn last_answer(&self) -> Option<String> {
        let Some(Message::Assistant { blocks, .. }) = self.messages.last() else {
            return None;
        };
        let text = text_of(blocks);
        (!text.trim().is_empty()).then_some(text)
    }

    pub(super) fn tell_subagents(&self, mode: PermissionMode) {
        if let Phase::Tools(batch) = &self.phase {
            batch.subagents.tell(|| Order::Mode(mode));
        }
    }

    pub(super) fn start_subagents(&mut self) {
        let Phase::Tools(batch) = &mut self.phase else {
            return;
        };
        let tasks: Vec<(String, Task)> = batch
            .calls
            .iter()
            .filter(|call| call.name == AGENT_TOOL)
            .filter_map(|call| {
                agent::parse(&call.arguments)
                    .ok()
                    .map(|task| (call.id.clone(), task))
            })
            .collect();
        batch.subagents.queued.extend(tasks);
        self.launch_queued();
    }

    fn launch_queued(&mut self) {
        loop {
            let Phase::Tools(batch) = &mut self.phase else {
                return;
            };
            if batch.subagents.running.len() >= MAX_SUBAGENTS {
                return;
            }
            let Some((call, task)) = batch.subagents.queued.pop_front() else {
                return;
            };
            self.launch(call, task);
        }
    }

    fn subagent_start(&self, session: SessionFile) -> Start {
        Start {
            setup: Setup {
                system: agent::system(&self.setup.system),
                tools: agent::tools(&self.setup.tools),
                ..self.setup.clone()
            },
            session,
            messages: Vec::new(),
            model: self.model.clone(),
            effort: self.effort,
            mode: self.mode,
            snapshots: self.snapshots.clone(),
            prompts: 0,
            turn: self.turn,
        }
    }

    fn launch(&mut self, call: String, task: Task) {
        self.emit(ChatEvent::Tool(ToolCall {
            id: Some(ToolUseId::from(call.as_str())),
            name: AGENT_LABEL.to_owned(),
            summary: task.description.clone(),
            edit: None,
        }));
        let files = subagent_files(&self.session.subagents, &call);
        let opened = SessionFile::create(&files.record, new_session_id()).and_then(|session| {
            write_meta(&files, &meta(&call, &task, false))?;
            Ok::<_, io::Error>(session)
        });
        let start = opened.map(|session| self.subagent_start(session));
        let Phase::Tools(batch) = &mut self.phase else {
            return;
        };
        let start = match start {
            Ok(start) => start,
            Err(error) => {
                let failed = Ran {
                    content: error.to_string(),
                    failed: true,
                    edit: None,
                };
                batch.subagents.reports.insert(call, failed);
                return;
            }
        };
        let (orders, inbox) = channel();
        orders
            .send(Inbound::Order(Order::Send(Prompt::from(
                task.prompt.as_str(),
            ))))
            .unwrap_or_default();
        batch.subagents.running.insert(call.clone(), orders.clone());
        let allowed = self.allowed.clone();
        let parent = self.inbound.clone();
        thread::spawn(move || {
            let report = run_subagent(start, allowed, orders, &inbox, parent.clone(), call.clone());
            write_meta(&files, &meta(&call, &task, true)).unwrap_or_default();
            parent
                .send(Inbound::Subagent {
                    call,
                    news: SubagentNews::Finished(report),
                })
                .unwrap_or_default();
        });
    }

    pub(super) fn subagent_news(&mut self, call: String, news: SubagentNews) {
        let Phase::Tools(batch) = &mut self.phase else {
            return;
        };
        if !batch.subagents.running.contains_key(&call) {
            return;
        }
        match news {
            SubagentNews::Asked(mut ask) => {
                let outer = format!("{call}{ASK_ID_SEPARATOR}{}", ask.request_id);
                let inner = std::mem::replace(&mut ask.request_id, outer.clone());
                batch
                    .subagents
                    .asks
                    .insert(outer, RoutedAsk { call, inner });
                (self.sink)(LiveEvent::Ask(ask));
            }
            SubagentNews::Finished(report) => {
                batch.subagents.running.remove(&call);
                batch.subagents.asks.retain(|_, routed| routed.call != call);
                batch.subagents.reports.insert(call, report);
                self.launch_queued();
                if let Phase::Tools(batch) = &self.phase {
                    if batch.waits_on_subagent() {
                        self.advance_batch();
                    }
                }
            }
        }
    }
}
