use futures::channel::oneshot;
use krad_domain::UsageBudget;
use krad_protocol::{
    BusSnapshot, Command, ErrorCode, Event, EventKind, Reply, RequestId, RuntimeCommand, Scope,
    Seq, Snapshot,
};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::app_events::{AppEvents, EventsSetup};
use crate::attention::Attention;
use crate::bus::{open, Bus, BusSetup};
use crate::handle::RuntimeHandle;
use crate::harness_health::Health;
use crate::input::Input;
use crate::jobs::{JobDone, Jobs};
use crate::models::Models;
use crate::subscribers::Subscribers;
use crate::usage::Usage;

const THREAD_NAME: &str = "krad-runtime";
const INPUT_CAPACITY: usize = 1024;
const TICK_INTERVAL: Duration = Duration::from_millis(100);
const UNSUPPORTED_COMMAND: &str = "this runtime does not know the command";
const NO_BUS: &str = "this runtime serves no agent bus";

#[derive(Clone, Debug, Default)]
pub struct RuntimeSetup {
    pub bus: Option<BusSetup>,
    pub events: Option<EventsSetup>,
}

#[derive(Debug)]
pub struct Runtime {
    seq: Seq,
    outbox: Vec<EventKind>,
    subscribers: Subscribers,
    bus: Option<Bus>,
    events: AppEvents,
    attention: Attention,
    jobs: Jobs,
    health: Health,
    usage: Usage,
    models: Models,
    stopped: Arc<AtomicBool>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Flow {
    Running,
    Stopping,
}

impl Runtime {
    /// # Errors
    ///
    /// Fails when the runtime's thread cannot be spawned.
    #[tracing::instrument(skip_all)]
    pub fn start(setup: RuntimeSetup) -> io::Result<RuntimeHandle> {
        let (inbox, input) = sync_channel(INPUT_CAPACITY);
        let stopped = Arc::new(AtomicBool::new(false));
        let access = setup.bus.and_then(|bus| {
            open(bus, inbox.clone(), Arc::clone(&stopped))
                .inspect_err(|error| tracing::warn!(%error, "the agent bus could not start"))
                .ok()
                .map(Arc::new)
        });
        let events = AppEvents::start(setup.events, &inbox);
        let usage_budget = Arc::new(UsageBudget::default());
        let usage = Usage::new(Arc::clone(&usage_budget));
        let runtime = Self::idle(access.clone(), events, usage, &inbox, stopped);
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || runtime.run(&input))?;
        tracing::info!("runtime started");
        Ok(RuntimeHandle::new(inbox, access, usage_budget))
    }

    fn idle(
        access: Option<Arc<crate::bus::BusAccess>>,
        events: AppEvents,
        usage: Usage,
        inbox: &SyncSender<Input>,
        stopped: Arc<AtomicBool>,
    ) -> Self {
        Self {
            seq: Seq::default(),
            outbox: Vec::new(),
            subscribers: Subscribers::default(),
            bus: access.map(|access| Bus::new(access, inbox.clone())),
            events,
            attention: Attention::default(),
            jobs: Jobs::new(inbox.clone()),
            health: Health::default(),
            usage,
            models: Models::default(),
            stopped,
        }
    }

    fn run(mut self, input: &Receiver<Input>) {
        let mut next_tick = Instant::now() + TICK_INTERVAL;
        loop {
            let wait = next_tick.saturating_duration_since(Instant::now());
            let mut batch = match input.recv_timeout(wait) {
                Ok(first) => vec![first],
                Err(RecvTimeoutError::Timeout) => Vec::new(),
                Err(RecvTimeoutError::Disconnected) => break,
            };
            batch.extend(input.try_iter());
            if Instant::now() >= next_tick {
                next_tick = Instant::now() + TICK_INTERVAL;
                batch.push(Input::Tick);
            }
            let mut flow = Flow::Running;
            for input in batch {
                if self.apply(input) == Flow::Stopping {
                    flow = Flow::Stopping;
                }
            }
            self.flush();
            if flow == Flow::Stopping {
                break;
            }
        }
        self.stopped.store(true, Ordering::Release);
        tracing::info!("runtime stopped");
    }

    fn apply(&mut self, input: Input) -> Flow {
        match input {
            Input::Command { id, command, reply } => {
                let answer = self.execute(id, &command);
                answer_to(reply, answer);
            }
            Input::Subscribe(feed) => self.subscribers.add(feed),
            Input::Snapshot { scope, reply } => answer_to(reply, self.snapshot(scope)),
            Input::Stop => return Flow::Stopping,
            Input::WebhookTested(tested) => self.outbox.push(EventKind::WebhookTested(tested)),
            Input::Tick => {
                if let Some(bus) = &mut self.bus {
                    bus.tick(&mut self.outbox);
                }
                self.usage.tick(&self.jobs, &mut self.outbox);
            }
            Input::CheckHarnesses { place, probes } => {
                self.health
                    .check(&place, probes, &self.jobs, &mut self.outbox);
            }
            Input::PlaceUsage(place) => self.usage.place(place, &self.jobs, &mut self.outbox),
            Input::ListModels {
                host,
                harness,
                runner,
                cwd,
            } => self.models.list((host, harness), runner, cwd, &self.jobs),
            Input::JobDone(done) => self.job_done(done),
            Input::Bus(incoming) => {
                let Some(bus) = &mut self.bus else {
                    tracing::debug!("a bus request reached a runtime without a bus");
                    return Flow::Running;
                };
                bus.answer(incoming, &mut self.outbox);
            }
            bus_input => self.apply_bus(bus_input),
        }
        Flow::Running
    }

    fn apply_bus(&mut self, input: Input) {
        let Some(bus) = &mut self.bus else {
            return;
        };
        let outbox = &mut self.outbox;
        match input {
            Input::BusRespond {
                delegation,
                response,
            } => bus.respond(delegation, response),
            Input::BusMail { delegation, mail } => bus.post_mail(delegation, mail, outbox),
            Input::BusDropped(delegation) => bus.dropped(delegation),
            Input::Delegate(delegate) => bus.delegate_to(delegate),
            Input::ObserveAgent { key, seen, hint } => bus.observe(&key, seen, hint, outbox),
            Input::ReconcileAgents {
                environments,
                agents,
            } => bus.reconcile(&environments, agents, outbox),
            Input::Command { .. }
            | Input::Subscribe(_)
            | Input::Snapshot { .. }
            | Input::Bus(_)
            | Input::WebhookTested(_)
            | Input::CheckHarnesses { .. }
            | Input::PlaceUsage(_)
            | Input::ListModels { .. }
            | Input::JobDone(_)
            | Input::Tick
            | Input::Stop => {}
        }
    }

    fn job_done(&mut self, done: JobDone) {
        match done {
            JobDone::HarnessChecked {
                place,
                harness,
                health,
            } => self
                .health
                .checked(place, harness, health, &mut self.outbox),
            JobDone::ModelsListed { key, listed } => {
                self.models.listed(key, listed, &mut self.outbox);
            }
            JobDone::UsageRead { key, read } => self.usage.read(&key, read, &mut self.outbox),
        }
    }

    fn execute(&mut self, id: RequestId, command: &Command) -> Reply {
        match command {
            Command::Runtime(RuntimeCommand::Ping) => {
                self.outbox.push(EventKind::Pong { request: id });
                Reply::Accepted
            }
            Command::Bus(command) => {
                let Some(bus) = &mut self.bus else {
                    return refused(ErrorCode::Unsupported, NO_BUS);
                };
                bus.execute(command, &mut self.outbox)
                    .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND))
            }
            Command::Events(command) => self
                .events
                .execute(command, &mut self.outbox)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Attention(command) => self
                .attention
                .execute(command, &mut self.outbox)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Usage(command) => self
                .usage
                .execute(command, &self.jobs, &mut self.outbox)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Models(command) => self
                .models
                .execute(command, &mut self.outbox)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Runtime(RuntimeCommand::Unknown) | Command::Unknown => {
                refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)
            }
        }
    }

    fn flush(&mut self) {
        let events: Vec<Event> = self
            .outbox
            .drain(..)
            .map(|kind| {
                self.seq = self.seq.next();
                Event {
                    seq: self.seq,
                    kind,
                }
            })
            .collect();
        let parts = Parts {
            bus: self.bus.as_ref(),
            attention: &self.attention,
            health: &self.health,
            usage: &self.usage,
            models: &self.models,
        };
        let seq = self.seq;
        self.subscribers
            .publish(&events, || parts.snapshot(seq, Scope::Runtime));
    }

    fn snapshot(&self, scope: Scope) -> Snapshot {
        Parts {
            bus: self.bus.as_ref(),
            attention: &self.attention,
            health: &self.health,
            usage: &self.usage,
            models: &self.models,
        }
        .snapshot(self.seq, scope)
    }
}

struct Parts<'a> {
    bus: Option<&'a Bus>,
    attention: &'a Attention,
    health: &'a Health,
    usage: &'a Usage,
    models: &'a Models,
}

impl Parts<'_> {
    fn snapshot(&self, seq: Seq, scope: Scope) -> Snapshot {
        Snapshot {
            scope,
            seq,
            bus: self.bus.map_or_else(BusSnapshot::default, Bus::snapshot),
            attention: self.attention.snapshot(),
            health: self.health.snapshot(),
            usage: self.usage.snapshot(),
            models: self.models.snapshot(),
        }
    }
}

pub(crate) fn refused(code: ErrorCode, message: &str) -> Reply {
    Reply::Refused {
        code,
        message: message.to_owned(),
    }
}

fn answer_to<T: std::fmt::Debug>(reply: oneshot::Sender<T>, answer: T) {
    if let Err(unheard) = reply.send(answer) {
        tracing::trace!(?unheard, "a client left before its answer");
    }
}
