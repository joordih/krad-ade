mod action;
mod launch;
mod pump;
mod remote_pty;
mod terminal;
mod typed;
mod view;

pub use action::TerminalAction;
pub use launch::{
    remote_terminal, remote_terminal_opener, HostShell, OutputTap, TerminalLaunch, TerminalPlace,
};
pub use view::{ShownImage, TerminalFrame, TerminalStatus, TerminalView};

pub(crate) use typed::Typed;
pub(crate) use view::Shared;

use krad_domain::{ProcessRun, TerminalMeta};
use krad_protocol::{
    AgentRef, ErrorCode, EventKind, Reply, TerminalClassified, TerminalCommand, TerminalSummary,
};
use krad_session::{
    run_spawn_job, AnsiPalette, FirstInput, LiveSession, PtyInput, PumpMsg, PumpSink as _,
    SpawnJob, VtEngine, DEFAULT_COLS, DEFAULT_ROWS, THREAD_IO_NAME, XTERM_PALETTE,
};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver, SyncSender};
use std::sync::Arc;
use std::time::Instant;

use self::launch::Placed;
use self::pump::{PumpQueue, Pumped};
use self::terminal::{Generation, Opened, Terminal, Written};
use crate::input::Input;
use crate::runtime::refused;

const UNKNOWN_TERMINAL: &str = "this runtime runs no terminal of that name";
const ENGINE_FAILED: &str = "The terminal could not start";
const PLACEMENT_FAILED: &str = "The terminal could not start on the host";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Settling {
    No,
    Now,
}

pub(crate) struct Terminals {
    live: HashMap<AgentRef, Terminal>,
    pumped: Receiver<Pumped>,
    queue: PumpQueue,
    palette: AnsiPalette,
    generations: u64,
}

impl std::fmt::Debug for Terminals {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Terminals")
            .field("live", &self.live.len())
            .finish_non_exhaustive()
    }
}

impl Terminals {
    pub(crate) fn new(wake: SyncSender<Input>) -> Self {
        let (queue, pumped) = PumpQueue::open(wake);
        Self {
            live: HashMap::new(),
            pumped,
            queue,
            palette: XTERM_PALETTE,
            generations: 0,
        }
    }

    #[tracing::instrument(skip_all, fields(terminal = terminal.terminal.as_u64()))]
    pub(crate) fn open(
        &mut self,
        terminal: AgentRef,
        launch: TerminalLaunch,
        shared: Arc<Shared>,
        outbox: &mut Vec<EventKind>,
    ) {
        if let Some(replaced) = self.live.remove(&terminal) {
            replaced.kill();
        }
        self.generations += 1;
        let generation = Generation(self.generations);
        let mut engine = match VtEngine::new(DEFAULT_COLS.into(), DEFAULT_ROWS.into()) {
            Ok(engine) => engine,
            Err(error) => {
                let error = format!("{ENGINE_FAILED}: {error}");
                return Self::refuse(terminal, &shared, &error, outbox);
            }
        };
        engine.push_text_lines(&launch.prefill);
        let (writer_tx, inbox) = channel();
        if let Some(typed) = launch.typed {
            drop(writer_tx.send(PtyInput::Bytes(typed)));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let session = LiveSession {
            engine,
            writer_tx,
            killer: None,
            cancelled: Arc::clone(&cancelled),
            state: ProcessRun::Running,
            last_output: Instant::now(),
            cwd: None,
            first_input: FirstInput::default(),
        };
        let opened = Opened {
            generation,
            session,
            harness: launch.harness,
            shared,
            tap: launch.tap,
        };
        self.live.insert(terminal.clone(), Terminal::new(opened));
        let placed = Placed {
            program: launch.program,
            args: launch.args,
            cwd: launch.cwd,
            vars: launch.vars,
            opener: None,
        };
        let mut pump = self.queue.of(generation);
        let env = terminal.environment.clone();
        let id = terminal.terminal;
        let spawned = std::thread::Builder::new()
            .name(THREAD_IO_NAME.to_owned())
            .spawn(move || {
                let placed = match placed.place(launch.place) {
                    Ok(placed) => placed,
                    Err(error) => {
                        let failure = format!("{PLACEMENT_FAILED}: {error}");
                        pump.deliver(PumpMsg::SpawnFailed(env, id, failure));
                        return;
                    }
                };
                run_spawn_job(SpawnJob {
                    env,
                    id,
                    program: placed.program,
                    args: placed.args,
                    cwd: placed.cwd,
                    vars: placed.vars,
                    pump,
                    cancelled,
                    inbox,
                    opener: placed.opener,
                });
            });
        if let Err(error) = spawned {
            self.ended(&terminal, outbox, |live| live.fail(error.to_string()));
        }
    }

    pub(crate) fn refuse(
        terminal: AgentRef,
        shared: &Shared,
        error: &str,
        outbox: &mut Vec<EventKind>,
    ) {
        let run = ProcessRun::Failed {
            error: error.to_owned(),
        };
        shared.set_status(TerminalStatus {
            run: run.clone(),
            ..TerminalStatus::default()
        });
        outbox.push(EventKind::TerminalEnded(TerminalSummary { terminal, run }));
    }

    pub(crate) fn runs(&self, terminal: &AgentRef) -> bool {
        self.live
            .get(terminal)
            .is_some_and(|live| live.run().is_running())
    }

    pub(crate) fn close(&mut self, terminal: &AgentRef) -> bool {
        self.live.remove(terminal).map(Terminal::kill).is_some()
    }

    pub(crate) fn pumped(&mut self, outbox: &mut Vec<EventKind>) {
        while let Ok(Pumped { generation, msg }) = self.pumped.try_recv() {
            self.apply(generation, msg, outbox);
        }
    }

    fn apply(&mut self, generation: Generation, msg: PumpMsg, outbox: &mut Vec<EventKind>) {
        let (environment, terminal) = match &msg {
            PumpMsg::Output(env, id, _)
            | PumpMsg::Ready(env, id, ..)
            | PumpMsg::Exited(env, id, _)
            | PumpMsg::SpawnFailed(env, id, _) => (env.clone(), *id),
        };
        let key = AgentRef {
            environment,
            terminal,
        };
        if self
            .live
            .get(&key)
            .is_none_or(|live| live.generation != generation)
        {
            return;
        }
        match msg {
            PumpMsg::Output(_, _, bytes) => {
                let Some(live) = self.live.get_mut(&key) else {
                    return;
                };
                if let Some(state) = live.feed(&bytes) {
                    outbox.push(classified(key, state));
                }
            }
            PumpMsg::Ready(_, _, cwd, _, killer) => {
                if let Some(live) = self.live.get_mut(&key) {
                    live.ready(cwd, killer);
                }
            }
            PumpMsg::Exited(_, _, code) => self.ended(&key, outbox, |live| live.exit(code)),
            PumpMsg::SpawnFailed(_, _, error) => self.ended(&key, outbox, |live| live.fail(error)),
        }
    }

    fn ended(
        &mut self,
        key: &AgentRef,
        outbox: &mut Vec<EventKind>,
        end: impl FnOnce(&mut Terminal),
    ) {
        let Some(live) = self.live.get_mut(key) else {
            return;
        };
        end(live);
        outbox.push(EventKind::TerminalEnded(TerminalSummary {
            terminal: key.clone(),
            run: live.run().clone(),
        }));
    }

    pub(crate) fn execute(
        &mut self,
        command: TerminalCommand,
        outbox: &mut Vec<EventKind>,
    ) -> Option<Reply> {
        let terminal = match &command {
            TerminalCommand::Write { terminal, .. }
            | TerminalCommand::Resize { terminal, .. }
            | TerminalCommand::Scroll { terminal, .. }
            | TerminalCommand::Close { terminal } => terminal.clone(),
            TerminalCommand::Open { .. }
            | TerminalCommand::OpenMissing { .. }
            | TerminalCommand::Type { .. }
            | TerminalCommand::Unknown => return None,
        };
        if let TerminalCommand::Close { terminal } = command {
            return Some(if self.close(&terminal) {
                Reply::Accepted
            } else {
                refused(ErrorCode::NotFound, UNKNOWN_TERMINAL)
            });
        }
        let Some(live) = self.live.get_mut(&terminal) else {
            return Some(refused(ErrorCode::NotFound, UNKNOWN_TERMINAL));
        };
        match command {
            TerminalCommand::Write { bytes, by, .. } => {
                if live.write(bytes, by) == Written::Lost && !live.run().is_running() {
                    self.ended(&terminal, outbox, |_| {});
                }
            }
            TerminalCommand::Resize { size, .. } => live.resize(size),
            TerminalCommand::Scroll { scroll, .. } => live.scroll(scroll),
            TerminalCommand::Close { .. }
            | TerminalCommand::Open { .. }
            | TerminalCommand::OpenMissing { .. }
            | TerminalCommand::Type { .. }
            | TerminalCommand::Unknown => {}
        }
        Some(Reply::Accepted)
    }

    pub(crate) fn act(
        &mut self,
        terminal: &AgentRef,
        action: TerminalAction,
        outbox: &mut Vec<EventKind>,
    ) {
        let Some(live) = self.live.get_mut(terminal) else {
            action.unanswered();
            return;
        };
        if live.act(action) == Written::Lost && !live.run().is_running() {
            self.ended(terminal, outbox, |_| {});
        }
    }

    pub(crate) fn recolor(&mut self, palette: AnsiPalette) {
        if palette == self.palette {
            return;
        }
        self.palette = palette;
        self.live.values_mut().for_each(Terminal::recolor);
    }

    pub(crate) fn tick(&mut self, now: Instant, outbox: &mut Vec<EventKind>) {
        for (key, live) in &mut self.live {
            if let Some(state) = live.reclassify(now) {
                outbox.push(classified(key.clone(), state));
            }
        }
    }

    pub(crate) fn render(&mut self, outbox: &mut Vec<EventKind>) {
        for (key, live) in &mut self.live {
            if live.render(&self.palette) {
                outbox.push(EventKind::TerminalChanged {
                    terminal: key.clone(),
                });
            }
        }
    }

    pub(crate) fn metas(
        &mut self,
        now: Instant,
        settling: Settling,
        kept: impl Fn(&AgentRef) -> bool,
    ) -> Vec<(AgentRef, TerminalMeta)> {
        self.live
            .iter_mut()
            .filter(|(key, live)| kept(key) && live.meta_due(now, settling == Settling::Now))
            .map(|(key, live)| (key.clone(), live.meta(now)))
            .collect()
    }

    pub(crate) fn snapshot(&self) -> Vec<TerminalSummary> {
        self.live
            .iter()
            .map(|(terminal, live)| TerminalSummary {
                terminal: terminal.clone(),
                run: live.run().clone(),
            })
            .collect()
    }

    pub(crate) fn stop(&mut self) {
        self.live.drain().for_each(|(_, live)| live.kill());
    }
}

const fn classified(terminal: AgentRef, state: krad_domain::AgentState) -> EventKind {
    EventKind::TerminalClassified(TerminalClassified { terminal, state })
}
