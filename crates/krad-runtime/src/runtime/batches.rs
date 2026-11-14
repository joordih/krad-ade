use krad_protocol::{Command, Event, EventKind, RuntimeCommand, Scope};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::input::Input;
use crate::settings::AppSettings;
use crate::terminals::Settling;

use super::parts::Parts;
use super::{answer_to, Runtime};

const TICK_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Flow {
    Running,
    Stopping,
}

impl Runtime {
    pub(super) fn run(mut self, input: &Receiver<Input>) {
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
            Input::WatchTerminal { terminal, reply } => {
                answer_to(reply, self.terminals.watch(&terminal));
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

    pub(super) fn save_terminal_metas(&mut self, settling: Settling) {
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
        self.publish_session_events();
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
        if let Some(own) = &self.runners.own {
            self.runners.activity.heard(own, &events);
        }
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
