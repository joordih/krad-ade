use futures::channel::oneshot;
use krad_domain::UsageBudget;
use krad_protocol::{ErrorCode, Event, EventKind, Reply, Scope, Seq, Snapshot};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::app_events::{AppEvents, EventsSetup};
use crate::attention::Attention;
use crate::bus::{open, Bus, BusSetup};
use crate::chats::Chats;
use crate::environments::{Environments, EnvironmentsSetup};
use crate::gateway::Gateway;
use crate::handle::RuntimeHandle;
use crate::harness_health::Health;
use crate::hosts::Hosts;
use crate::input::Input;
use crate::jobs::{JobDone, Jobs};
use crate::launches::{LaunchedViews, Launches};
use crate::models::Models;
use crate::opencode::{OpencodeDirectory, OpencodeServers};
use crate::settings::{AppSettings, SettingsSetup};
use crate::subscribers::Subscribers;
use crate::terminals::{Settling, Terminals};
use crate::usage::Usage;

mod chatting;
mod commands;
mod hosting;
mod launching;
mod parts;

pub(crate) use self::launching::AttachedLine;
use self::parts::Parts;

const THREAD_NAME: &str = "krad-runtime";
const INPUT_CAPACITY: usize = 1024;
const TICK_INTERVAL: Duration = Duration::from_millis(100);
const UNSUPPORTED_COMMAND: &str = "this runtime does not know the command";
const NO_BUS: &str = "this runtime serves no agent bus";

#[derive(Clone, Debug, Default)]
pub struct RuntimeSetup {
    pub bus: Option<BusSetup>,
    pub events: Option<EventsSetup>,
    pub environments: EnvironmentsSetup,
    pub settings: SettingsSetup,
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
    environments: Environments,
    terminals: Terminals,
    hosts: Hosts,
    chats: Chats,
    gateway: Gateway,
    opencode: OpencodeServers,
    settings: AppSettings,
    launches: Launches,
    stopped: Arc<AtomicBool>,
}

struct Owned {
    events: AppEvents,
    usage: Usage,
    environments: Environments,
    hosts: Hosts,
    opencode: OpencodeDirectory,
    settings: AppSettings,
    views: LaunchedViews,
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
        let environments = Environments::start(setup.environments);
        let (hosts, links) = Hosts::new(access.clone());
        let opencode = OpencodeDirectory::default();
        let settings = AppSettings::start(setup.settings, &inbox);
        let views = LaunchedViews::default();
        let owned = Owned {
            events,
            usage,
            environments,
            hosts,
            opencode: opencode.clone(),
            settings,
            views: views.clone(),
        };
        let shared_access = access.clone();
        let runtime_inbox = inbox.clone();
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || Self::idle(shared_access, owned, &runtime_inbox, stopped).run(&input))?;
        tracing::info!("runtime started");
        Ok(RuntimeHandle::new(
            inbox,
            access,
            usage_budget,
            (links, opencode, views),
        ))
    }

    fn idle(
        access: Option<Arc<crate::bus::BusAccess>>,
        owned: Owned,
        inbox: &SyncSender<Input>,
        stopped: Arc<AtomicBool>,
    ) -> Self {
        Self {
            seq: Seq::default(),
            outbox: Vec::new(),
            subscribers: Subscribers::default(),
            bus: access.map(|access| Bus::new(access, inbox.clone())),
            events: owned.events,
            attention: Attention::default(),
            jobs: Jobs::new(inbox.clone()),
            health: Health::default(),
            usage: owned.usage,
            models: Models::default(),
            environments: owned.environments,
            terminals: Terminals::new(inbox.clone()),
            hosts: owned.hosts,
            chats: Chats::new(inbox.clone()),
            gateway: Gateway::default(),
            opencode: OpencodeServers::new(inbox.clone(), owned.opencode),
            settings: owned.settings,
            launches: Launches::new(owned.views, inbox.clone()),
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
            let unread = self.outbox.len();
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
            self.terminals.pumped(&mut self.outbox);
            self.hear_chats();
            self.hear_ended_terminals(unread);
            self.follow_connections(unread);
            self.flush();
            if flow == Flow::Stopping {
                break;
            }
        }
        self.terminals.stop();
        self.stopped.store(true, Ordering::Release);
        tracing::info!("runtime stopped");
    }

    fn apply(&mut self, input: Input) -> Flow {
        match input {
            Input::Command { id, command, reply } => {
                let answer = self.execute(id, command);
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
                self.terminals.tick(Instant::now(), &mut self.outbox);
                self.tick_hosts();
            }
            Input::OpenTerminal {
                terminal,
                launch,
                shared,
            } => self
                .terminals
                .open(terminal, launch, shared, &mut self.outbox),
            Input::TerminalAction { terminal, action } => {
                self.terminals.act(&terminal, action, &mut self.outbox);
            }
            Input::TerminalPalette(palette) => self.terminals.recolor(palette),
            Input::OpenSignIn { terminal, shared } => self.open_sign_in(terminal, shared),
            Input::MachineChannel {
                environment,
                channel,
            } => self.hand_machine_channel(&environment, channel),
            Input::Wake => {}
            chat @ (Input::OpenChat { .. }
            | Input::CloseChat(_)
            | Input::ChatPage { .. }
            | Input::MentionIndex { .. }
            | Input::ForgetMentionIndexes
            | Input::StartLiveChat(_)
            | Input::Opencode(_)) => {
                self.apply_chat_input(chat);
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
            Input::Settle(reply) => {
                self.save_terminal_metas(Settling::Now);
                let settled = Arc::new(Settled(Some(reply)));
                let waiting = Arc::clone(&settled);
                self.settings.settle(move || drop(waiting));
                self.environments.settle(move || drop(settled));
            }
            Input::SettingsNotSaved(error) => AppSettings::not_saved(error, &mut self.outbox),
            Input::OfferMemoryTools(offered) => self.offer_memory_tools(offered),
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
            host_job => self.host_job_done(host_job),
        }
    }

    fn save_terminal_metas(&mut self, settling: Settling) {
        let environments = &self.environments;
        let metas = self.terminals.metas(Instant::now(), settling, |terminal| {
            environments.knows(&terminal.environment)
        });
        for (terminal, meta) in metas {
            self.environments.put_meta(terminal, meta, &mut self.outbox);
        }
    }

    fn flush(&mut self) {
        self.save_terminal_metas(Settling::No);
        self.environments.flush();
        self.settings.flush();
        self.terminals.render(&mut self.outbox);
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
            environments: &self.environments,
            terminals: &self.terminals,
            hosts: &self.hosts,
            chats: &self.chats,
            opencode: &self.opencode,
            gateway: &self.gateway,
            settings: &self.settings,
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
            environments: &self.environments,
            terminals: &self.terminals,
            hosts: &self.hosts,
            chats: &self.chats,
            opencode: &self.opencode,
            gateway: &self.gateway,
            settings: &self.settings,
        }
        .snapshot(self.seq, scope)
    }
}

#[derive(Debug)]
struct Settled(Option<oneshot::Sender<()>>);

impl Drop for Settled {
    fn drop(&mut self) {
        if let Some(reply) = self.0.take() {
            answer_to(reply, ());
        }
    }
}

pub(crate) fn refused(code: ErrorCode, message: &str) -> Reply {
    Reply::Refused {
        code,
        message: message.to_owned(),
    }
}

pub(super) fn answer_to<T: std::fmt::Debug>(reply: oneshot::Sender<T>, answer: T) {
    if let Err(unheard) = reply.send(answer) {
        tracing::trace!(?unheard, "a client left before its answer");
    }
}
