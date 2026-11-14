use krad_domain::UsageBudget;
use krad_protocol::{Command, Event, EventKind, RuntimeCommand, Scope, Seq};
use krad_store::{ChatImages, Secrets};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::adapters::AcpAdapters;
use crate::app_events::AppEvents;
use crate::attention::Attention;
use crate::bus::{open, Bus};
use crate::chats::{Chats, EditorBuffers};
use crate::cloud::CloudSessions;
use crate::cloud_runs::CloudRuns;
use crate::code_hosts::CodeHostWatch;
use crate::environments::Environments;
use crate::file_asks::Uploads;
use crate::gateway::Gateway;
use crate::git::Git;
use crate::handle::RuntimeHandle;
use crate::harness_health::Health;
use crate::hosts::Hosts;
use crate::input::Input;
use crate::jobs::Jobs;
use crate::kraddies::{Kraddies, KraddiesSetup};
use crate::launches::{LaunchedViews, Launches};
use crate::models::Models;
use crate::opencode::{OpencodeDirectory, OpencodeServers};
use crate::reviews::Reviews;
use crate::runners::Runners;
use crate::settings::AppSettings;
use crate::subscribers::Subscribers;
use crate::terminals::{Settling, Terminals};
use crate::typing::Typist;
use crate::usage::Usage;
use crate::workers::Workers;

mod acp_listing;
mod agent_commands;
mod archive_files;
mod bus_git;
mod chat_images;
mod chatting;
mod ci_fixing;
mod cloud_nodes;
mod cloud_results;
mod clouding;
mod code_host_watch;
mod commands;
mod digesting;
mod file_asks;
mod finished_jobs;
mod following;
mod git_acts;
mod gitting;
mod harness_spots;
mod history_asks;
mod hooking;
mod host_asks;
mod hosting;
mod keystrokes;
mod kraddie_bus;
mod kraddie_chats;
mod kraddie_commands;
mod kraddie_following;
mod kraddie_rounds;
mod kraddie_runs;
mod kraddie_worktrees;
mod launching;
mod parts;
mod removing;
mod replies;
mod reviewing;
mod runner_lines;
mod runner_logins;
mod runner_return;
mod runner_webhooks;
mod running;
mod session_changes;
mod settling;
mod setup;
mod sign_in_launches;
mod signing_in;
mod terminal_targets;
mod ticking;
mod variant_bus;
mod variant_picks;
mod variant_progress;
mod variant_runs;
mod worker_worktrees;
mod working;

pub(crate) use self::chat_images::ChatImageDone;
pub(crate) use self::ci_fixing::PendingFix;
pub(crate) use self::kraddie_following::LinesCounted;
pub(crate) use self::kraddie_runs::RunPlan;
pub(crate) use self::launching::AttachedLine;
use self::parts::Parts;
pub(crate) use self::replies::{answer_to, refused};
pub use self::setup::RuntimeSetup;
pub(crate) use self::sign_in_launches::SignInEnvRead;

const THREAD_NAME: &str = "krad-runtime";
const INPUT_CAPACITY: usize = 1024;
const TICK_INTERVAL: Duration = Duration::from_millis(100);
const UNSUPPORTED_COMMAND: &str = "this runtime does not know the command";
const NO_BUS: &str = "this runtime serves no agent bus";

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
    adapters: AcpAdapters,
    buffers: EditorBuffers,
    code_hosts: CodeHostWatch,
    reviews: Reviews,
    runners: Runners,
    cloud: CloudSessions,
    cloud_runs: CloudRuns,
    gateway: Gateway,
    opencode: OpencodeServers,
    settings: AppSettings,
    launches: Launches,
    workers: Workers,
    git: Git,
    kraddies: Kraddies,
    typist: Typist,
    secrets: Secrets,
    chat_images: Option<ChatImages>,
    uploads: Uploads,
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
    kraddies: KraddiesSetup,
    secrets: Secrets,
    chat_images: Option<ChatImages>,
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
        let events = AppEvents::start(setup.events, &setup.secrets, &inbox);
        let usage = Usage::new(Arc::new(UsageBudget::default()));
        let environments = Environments::start(setup.environments);
        let settings = AppSettings::start(setup.settings, &inbox);
        let host_context = setup.host_tools.context(settings.current.host_timeouts);
        let (hosts, links) = Hosts::new(access.clone(), host_context);
        let opencode = OpencodeDirectory::default();
        let views = LaunchedViews::default();
        let owned = Owned {
            events,
            usage,
            environments,
            hosts,
            opencode: opencode.clone(),
            settings,
            views: views.clone(),
            kraddies: setup.kraddies,
            secrets: setup.secrets,
            chat_images: setup.chat_images,
        };
        let secrets = owned.secrets.clone();
        let shared_access = access.clone();
        let runtime_inbox = inbox.clone();
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || Self::idle(shared_access, owned, &runtime_inbox, stopped).run(&input))?;
        tracing::info!("runtime started");
        Ok(RuntimeHandle::new(
            inbox,
            access,
            (links, opencode, views, secrets),
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
            adapters: AcpAdapters::default(),
            buffers: EditorBuffers::default(),
            code_hosts: CodeHostWatch::default(),
            reviews: Reviews::default(),
            runners: Runners::default(),
            cloud: CloudSessions::default(),
            cloud_runs: CloudRuns::default(),
            gateway: Gateway::default(),
            opencode: OpencodeServers::new(inbox.clone(), owned.opencode),
            settings: owned.settings,
            launches: Launches::new(owned.views, inbox.clone()),
            workers: Workers::default(),
            git: Git::default(),
            kraddies: Kraddies::start(owned.kraddies, inbox.clone()),
            typist: Typist::default(),
            secrets: owned.secrets,
            chat_images: owned.chat_images,
            uploads: Uploads::default(),
            stopped,
        }
    }

    fn run(mut self, input: &Receiver<Input>) {
        let mut next_tick = Instant::now() + TICK_INTERVAL;
        self.catch_up_kraddies();
        self.flush();
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
            self.keep_chat_resumes();
            self.follow_workers();
            self.follow_kraddie_runs();
            self.follow_variants();
            self.digest_ended_chats();
            self.hear_kraddie_events(unread);
            self.settle_waiters();
            self.hear_ended_terminals(unread);
            self.follow_connections(unread);
            self.settle_cloud_runs();
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
            Input::Command {
                command: Command::Runtime(RuntimeCommand::Settle),
                reply,
                ..
            } => self.settle(reply),
            Input::Command { id, command, reply } => {
                let answer = self.execute(id, command);
                answer_to(reply, answer);
            }
            Input::Subscribe { feed, since } => self.subscribers.add(feed, since, self.seq),
            Input::Snapshot { scope, reply } => answer_to(reply, self.snapshot(scope)),
            Input::Stop => return Flow::Stopping,
            Input::WebhookTested(tested) => self.outbox.push(EventKind::WebhookTested(tested)),
            Input::Tick => self.tick(),
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
            Input::Runner(runner) => self.apply_runner_input(runner),
            Input::MachineChannel {
                environment,
                channel,
            } => self.hand_machine_channel(&environment, channel),
            Input::Wake => {}
            chat @ (Input::OpenChat { .. } | Input::OfferBuffers { .. } | Input::Opencode(_)) => {
                self.apply_chat_input(chat);
            }
            Input::JobDone(done) => self.job_done(done),
            Input::SettingsNotSaved(error) => AppSettings::not_saved(error, &mut self.outbox),
            Input::KraddieNotSaved(error) => self.outbox.push(EventKind::KraddieNotSaved { error }),
            Input::DigestHeard { chat, reply } => self.digest_heard(&chat, reply),
            Input::Bus(incoming) => self.answer_bus(incoming),
            bus_input => self.apply_bus(bus_input),
        }
        Flow::Running
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
        self.kraddies.flush(&mut self.outbox);
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
            adapters: &self.adapters,
            opencode: &self.opencode,
            gateway: &self.gateway,
            settings: &self.settings,
            git: &self.git,
            kraddies: &self.kraddies,
            runners: &self.runners,
        };
        let seq = self.seq;
        self.subscribers
            .publish(&events, || parts.snapshot(seq, Scope::Runtime));
    }
}
