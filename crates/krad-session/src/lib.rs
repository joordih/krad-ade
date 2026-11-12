pub use krad_pty::{
    clear_screen_prefix, default_shell, find_program, login_shell_env, wsl_shell, wsl_shell_in,
    GridSize, PtyError, PtyInput, PtyKill, PtyOpener, PtyResize, PtyWait, SharedKiller,
    SharedMaster, SpawnedPty, DEFAULT_COLS, DEFAULT_ROWS, SHELL_QUOTING, TERMINAL_ENV,
};
pub use krad_term::{
    block, AnsiPalette, Block, CellPixels, CellPoint, CellRect, CursorShape, ImageDepth,
    ImagePixels, KeyModifiers, MouseAction, SearchDirection, SpanStyle, TermCursor, TermImage,
    TermRow, TermScreen, TermSpan, VtEngine, WheelInput, ANSI_COLORS, CELL_STEPS, XTERM_PALETTE,
};

use krad_domain::{EnvironmentName, ProcessRun, TerminalId};
use krad_pty::{run_reader_loop, run_writer_loop, spawn_pty, watch_exit, SpawnRequest};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

pub const THREAD_IO_NAME: &str = "krad-ade-pty";
const WRITER_THREAD_FAILED: &str = "writer thread unavailable";

pub trait PumpSink: Clone + Send + 'static {
    fn deliver(&mut self, msg: PumpMsg) -> bool;
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
}

#[derive(Clone, Debug, Default)]
pub struct FirstInput(Arc<AtomicU8>);

const INPUT_UNTOUCHED: u8 = 0;
const INPUT_FROM_USER: u8 = 1;
const INPUT_FROM_APP: u8 = 2;

impl FirstInput {
    #[must_use]
    pub fn claim_for_app(&self) -> bool {
        self.0
            .compare_exchange(
                INPUT_UNTOUCHED,
                INPUT_FROM_APP,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub fn user_typed(&self) {
        self.0
            .compare_exchange(
                INPUT_UNTOUCHED,
                INPUT_FROM_USER,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .ok();
    }
}

pub struct LiveSession {
    pub engine: VtEngine,
    pub writer_tx: Sender<PtyInput>,
    pub killer: Option<SharedKiller>,
    pub cancelled: Arc<AtomicBool>,
    pub state: ProcessRun,
    pub last_output: Instant,
    pub cwd: Option<String>,
    pub first_input: FirstInput,
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

pub struct SpawnJob<S> {
    pub env: EnvironmentName,
    pub id: TerminalId,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub vars: Vec<(String, String)>,
    pub pump: S,
    pub cancelled: Arc<AtomicBool>,
    pub inbox: Receiver<PtyInput>,
    pub opener: Option<PtyOpener>,
}

pub fn run_spawn_job<S: PumpSink>(job: SpawnJob<S>) {
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
        opener,
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
    let opened = match opener {
        Some(open) => open(request.cols, request.rows),
        None => spawn_pty(&request),
    };
    let spawned = match opened {
        Ok(spawned) => spawned,
        Err(error) => {
            pump.deliver(PumpMsg::SpawnFailed(env, id, error.to_string()));
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
            pump.deliver(PumpMsg::SpawnFailed(env, id, error.to_string()));
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
        pump.deliver(PumpMsg::SpawnFailed(
            env,
            id,
            WRITER_THREAD_FAILED.to_owned(),
        ));
        return;
    }
    pump.deliver(PumpMsg::Ready(env.clone(), id, cwd_text, pid, killer));
    let mut pump_output = pump.clone();
    run_reader_loop(&mut reader, &cancelled, |bytes| {
        pump_output.deliver(PumpMsg::Output(env.clone(), id, bytes.to_vec()))
    });
    let code = watcher.join().ok().flatten();
    pump.deliver(PumpMsg::Exited(env, id, code));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whoever_types_first_owns_the_fresh_prompt() {
        let first = FirstInput::default();
        assert!(first.claim_for_app());
        first.user_typed();
        assert!(!first.claim_for_app());
        let typed = FirstInput::default();
        typed.user_typed();
        assert!(!typed.claim_for_app());
    }
}
