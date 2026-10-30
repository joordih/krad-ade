pub use krad_pty::{
    clear_screen_prefix, default_shell, find_program, login_shell_env, wsl_shell, wsl_shell_in,
    GridSize, PtyInput, SharedKiller, DEFAULT_COLS, DEFAULT_ROWS, SHELL_QUOTING,
};
pub use krad_term::{
    block, AnsiPalette, Block, CellPixels, CellPoint, CellRect, CursorShape, ImageDepth,
    ImagePixels, KeyModifiers, MouseAction, SearchDirection, SpanStyle, TermCursor, TermImage,
    TermRow, TermSpan, VtEngine, WheelInput, ANSI_COLORS, CELL_STEPS, XTERM_PALETTE,
};

use futures::channel::mpsc::{channel, Receiver as PumpReceiver, Sender as PumpSender};
use futures::executor::block_on;
use futures::SinkExt as _;
use krad_domain::{AgentPreset, EnvironmentName, TerminalId};
use krad_pty::{run_reader_loop, run_writer_loop, spawn_pty, watch_exit, SpawnRequest, SpawnedPty};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const TICK_SECS: u64 = 1;
pub const THREAD_IO_NAME: &str = "krad-ade-pty";
pub const THREAD_TICK_NAME: &str = "krad-ade-tick";
pub const PUMP_CAPACITY: usize = 256;
const WRITER_THREAD_FAILED: &str = "writer thread unavailable";

#[must_use]
pub fn pump_channel() -> (PumpSender<PumpMsg>, PumpReceiver<PumpMsg>) {
    channel(PUMP_CAPACITY)
}

fn deliver(pump: &mut PumpSender<PumpMsg>, msg: PumpMsg) -> bool {
    block_on(pump.send(msg)).is_ok()
}

pub enum PumpMsg {
    Output(EnvironmentName, TerminalId, Vec<u8>),
    Ready(
        EnvironmentName,
        TerminalId,
        Option<String>,
        Option<u32>,
        SharedKiller,
    ),
    Exited(EnvironmentName, TerminalId, Option<i32>),
    SpawnFailed(EnvironmentName, TerminalId, String),
    Tick,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SessionState {
    #[default]
    Running,
    Exited(Option<i32>),
    Failed(String),
}

impl SessionState {
    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }
}

pub struct LiveSession {
    pub engine: VtEngine,
    pub writer_tx: Sender<PtyInput>,
    pub killer: Option<SharedKiller>,
    pub cancelled: Arc<AtomicBool>,
    pub state: SessionState,
    pub last_output: Instant,
    pub spawned_at: Instant,
    pub is_agent: bool,
    pub harness: Option<AgentPreset>,
    pub hooked: bool,
    pub activity: Option<String>,
    pub cwd: Option<String>,
}

impl LiveSession {
    pub fn feed(&mut self, bytes: &[u8]) {
        self.engine.push(bytes);
        self.last_output = Instant::now();
        let replies = self.engine.take_replies();
        if !replies.is_empty() {
            drop(self.writer_tx.send(PtyInput::Bytes(replies)));
        }
    }

    pub fn mark_exited(&mut self, code: Option<i32>) {
        if !matches!(self.state, SessionState::Failed(_)) {
            self.state = SessionState::Exited(code);
        }
    }

    pub fn resize_grid(&mut self, size: GridSize) {
        let cell = CellPixels {
            width: u32::from(size.cell_width_px),
            height: u32::from(size.cell_height_px),
        };
        let resized = self
            .engine
            .resize(usize::from(size.cols), usize::from(size.rows), cell);
        if resized {
            drop(self.writer_tx.send(PtyInput::Resize(size)));
        }
    }
}

pub struct SpawnJob {
    pub env: EnvironmentName,
    pub id: TerminalId,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub vars: Vec<(String, String)>,
    pub pump: PumpSender<PumpMsg>,
    pub cancelled: Arc<AtomicBool>,
    pub inbox: Receiver<PtyInput>,
}

pub fn run_spawn_job(job: SpawnJob) {
    let SpawnJob {
        env,
        id,
        program,
        args,
        cwd,
        vars,
        pump,
        cancelled,
        inbox,
    } = job;
    let mut pump = pump;
    let cwd_text = cwd.as_ref().map(|path| path.to_string_lossy().into_owned());
    let request = SpawnRequest {
        program,
        args,
        cwd,
        env: vars,
        cols: DEFAULT_COLS,
        rows: DEFAULT_ROWS,
    };
    let spawned = match spawn_pty(&request) {
        Ok(spawned) => spawned,
        Err(error) => {
            deliver(&mut pump, PumpMsg::SpawnFailed(env, id, error.to_string()));
            return;
        }
    };
    if cancelled.load(Ordering::SeqCst) {
        spawned.killer.kill();
        return;
    }
    let SpawnedPty {
        mut reader,
        mut writer,
        child,
        killer,
        pid,
        master,
    } = spawned;
    let watcher = match watch_exit(child, master.clone()) {
        Ok(watcher) => watcher,
        Err(error) => {
            killer.kill();
            deliver(&mut pump, PumpMsg::SpawnFailed(env, id, error.to_string()));
            return;
        }
    };
    let writer_thread = std::thread::Builder::new()
        .name(THREAD_IO_NAME.to_owned())
        .spawn(move || {
            run_writer_loop(&mut writer, inbox, |size| {
                drop(master.resize(size));
            });
        });
    if writer_thread.is_err() {
        killer.kill();
        deliver(
            &mut pump,
            PumpMsg::SpawnFailed(env, id, WRITER_THREAD_FAILED.to_owned()),
        );
        return;
    }
    deliver(
        &mut pump,
        PumpMsg::Ready(env.clone(), id, cwd_text, pid, killer),
    );
    let mut pump_output = pump.clone();
    run_reader_loop(&mut reader, &cancelled, |bytes| {
        deliver(
            &mut pump_output,
            PumpMsg::Output(env.clone(), id, bytes.to_vec()),
        )
    });
    let code = watcher.join().ok().flatten();
    deliver(&mut pump, PumpMsg::Exited(env, id, code));
}

pub fn spawn_ticker(mut pump: PumpSender<PumpMsg>) {
    let outcome = std::thread::Builder::new()
        .name(THREAD_TICK_NAME.to_owned())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(TICK_SECS));
            if !deliver(&mut pump, PumpMsg::Tick) {
                break;
            }
        });
    drop(outcome);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(state: SessionState) -> LiveSession {
        let (writer_tx, _writer_rx) = std::sync::mpsc::channel();
        LiveSession {
            engine: VtEngine::new(DEFAULT_COLS.into(), DEFAULT_ROWS.into())
                .unwrap_or_else(|error| unreachable!("{error}")),
            writer_tx,
            killer: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            state,
            last_output: Instant::now(),
            spawned_at: Instant::now(),
            is_agent: false,
            harness: None,
            hooked: false,
            activity: None,
            cwd: None,
        }
    }

    #[test]
    fn exit_ends_a_running_session_but_keeps_a_failure() {
        let mut running = session(SessionState::Running);
        running.mark_exited(Some(1));
        assert_eq!(running.state, SessionState::Exited(Some(1)));

        let failure = SessionState::Failed("writer lost".to_owned());
        let mut failed = session(failure.clone());
        failed.mark_exited(Some(0));
        assert_eq!(failed.state, failure);
    }
}
