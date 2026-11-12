use krad_domain::{CloudProcessRun, JournalOffset};
use portable_pty::MasterPty;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use super::journal::{append, Journal};
use super::wire::Frame;
use crate::mux::TerminalSize;

const CHUNK_BYTES: usize = 32 * 1024;
const SIGNAL_EXIT_BASE: i32 = 128;
const KILL_PROGRAM: &str = "kill";
const TERMINATE_SIGNAL: &str = "-TERM";
const KILL_SIGNAL: &str = "-KILL";
const PROBE_SIGNAL: &str = "-0";
const END_OF_OPTIONS: &str = "--";
const KILL_GRACE: Duration = Duration::from_secs(2);
const PUMP_THREAD: &str = "krad-cloud-pump";
pub(super) const SPAWN_FAILED: &str = "the cloud runner could not start the program";

pub(super) type Input = Box<dyn Write + Send>;
pub(super) type Terminal = Box<dyn MasterPty + Send>;

#[derive(Debug, Default)]
struct Progress {
    written: u64,
    run: CloudProcessRun,
}

pub(super) struct Tracked {
    pub(super) journal: Journal,
    progress: Mutex<Progress>,
    changed: Condvar,
    input: Mutex<Option<Input>>,
    terminal: Option<Mutex<Terminal>>,
    group: Option<u32>,
}

impl Tracked {
    pub(super) fn finished(journal: Journal) -> io::Result<Self> {
        let written = journal.len()?.0;
        let run = CloudProcessRun::Ended(journal.exit_code());
        Ok(Self {
            journal,
            progress: Mutex::new(Progress { written, run }),
            changed: Condvar::new(),
            input: Mutex::new(None),
            terminal: None,
            group: None,
        })
    }

    pub(super) fn live(
        journal: Journal,
        input: Input,
        terminal: Option<Terminal>,
        group: Option<u32>,
    ) -> Self {
        if let Some(pid) = group {
            if let Err(error) = journal.record_pid(pid) {
                tracing::debug!(%error, "could not record a cloud process id");
            }
        }
        Self {
            journal,
            progress: Mutex::new(Progress::default()),
            changed: Condvar::new(),
            input: Mutex::new(Some(input)),
            terminal: terminal.map(Mutex::new),
            group,
        }
    }

    pub(super) fn adopted(journal: Journal, input: Input, group: u32) -> Self {
        Self {
            journal,
            progress: Mutex::new(Progress::default()),
            changed: Condvar::new(),
            input: Mutex::new(Some(input)),
            terminal: None,
            group: Some(group),
        }
    }

    fn progress(&self) -> MutexGuard<'_, Progress> {
        self.progress.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn written(&self) -> JournalOffset {
        JournalOffset(self.progress().written)
    }

    pub(super) fn run(&self) -> CloudProcessRun {
        self.progress().run
    }

    pub(super) fn running_group(&self) -> Option<u32> {
        self.group
            .filter(|_| self.run() == CloudProcessRun::Running)
    }

    fn record_output(&self, bytes: usize) {
        self.progress().written += bytes as u64;
        self.changed.notify_all();
    }

    pub(super) fn observe_length(&self) {
        let Ok(length) = self.journal.len() else {
            return;
        };
        let mut progress = self.progress();
        if length.0 > progress.written {
            progress.written = length.0;
            self.changed.notify_all();
        }
    }

    pub(super) fn record_exit(&self, code: Option<i32>) {
        if let (Some(code), None) = (code, self.journal.exit_code()) {
            if let Err(error) = self.journal.record_exit(code) {
                tracing::debug!(%error, "could not record a cloud process exit code");
            }
        }
        self.progress().run = CloudProcessRun::Ended(code);
        self.changed.notify_all();
    }

    pub(super) fn resize(&self, size: TerminalSize) -> Option<io::Result<()>> {
        let terminal = self.terminal.as_ref()?;
        let terminal = terminal.lock().unwrap_or_else(PoisonError::into_inner);
        Some(
            terminal
                .resize(super::pty::pty_size(size))
                .map_err(io::Error::other),
        )
    }
}

pub(super) fn spawn_pump(
    tracked: &Arc<Tracked>,
    reader: impl Read + Send + 'static,
    output: File,
    wait: impl FnOnce() -> Option<i32> + Send + 'static,
) -> Result<(), &'static str> {
    let pumped = Arc::clone(tracked);
    thread::Builder::new()
        .name(PUMP_THREAD.to_owned())
        .spawn(move || {
            journal_output(&pumped, reader, output);
            pumped.record_exit(wait());
        })
        .map(drop)
        .map_err(|_| SPAWN_FAILED)
}

fn journal_output(tracked: &Tracked, mut reader: impl Read, mut output: File) {
    let mut chunk = vec![0; CHUNK_BYTES];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                if let Err(error) = append(&mut output, &chunk[..read]) {
                    tracing::warn!(%error, "could not write a cloud process journal");
                    break;
                }
                tracked.record_output(read);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

pub(super) fn forward_input(tracked: &Arc<Tracked>, mut stream: UnixStream) {
    let tracked = Arc::clone(tracked);
    let spawned = thread::Builder::new()
        .name(PUMP_THREAD.to_owned())
        .spawn(move || {
            let mut chunk = vec![0; CHUNK_BYTES];
            while let Ok(read) = stream.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                if tracked.run() != CloudProcessRun::Running {
                    break;
                }
                let mut input = tracked.input.lock().unwrap_or_else(PoisonError::into_inner);
                let Some(pipe) = input.as_mut() else { break };
                if pipe
                    .write_all(&chunk[..read])
                    .and_then(|()| pipe.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not forward input to a cloud process");
    }
}

pub(super) fn stream_output(
    tracked: &Tracked,
    from: JournalOffset,
    out: &mut UnixStream,
) -> io::Result<()> {
    let mut sent = from;
    loop {
        let (written, run) = {
            let mut progress = tracked.progress();
            while progress.written <= sent.0 && progress.run == CloudProcessRun::Running {
                progress = tracked
                    .changed
                    .wait(progress)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            (progress.written, progress.run)
        };
        while sent.0 < written {
            let chunk = tracked.journal.read(sent, CHUNK_BYTES)?;
            if chunk.is_empty() {
                break;
            }
            sent = sent.advanced(chunk.len());
            Frame::Data(chunk).write(out)?;
        }
        if let CloudProcessRun::Ended(code) = run {
            if sent.0 >= written {
                return Frame::Exit(code).write(out);
            }
        }
    }
}

pub(super) fn stop_group(group: u32) {
    signal_group(TERMINATE_SIGNAL, &format!("-{group}"));
    thread::sleep(KILL_GRACE);
    if group_alive(group) {
        signal_group(KILL_SIGNAL, &format!("-{group}"));
    }
}

pub(super) fn group_alive(group: u32) -> bool {
    Command::new(KILL_PROGRAM)
        .args([PROBE_SIGNAL, END_OF_OPTIONS, &format!("-{group}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(super) fn exit_code_of(status: ExitStatus) -> Option<i32> {
    status
        .code()
        .or_else(|| status.signal().map(|signal| SIGNAL_EXIT_BASE + signal))
}

fn signal_group(signal: &str, group: &str) {
    if let Err(error) = Command::new(KILL_PROGRAM)
        .args([signal, END_OF_OPTIONS, group])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        tracing::debug!(%error, "could not signal a cloud process group");
    }
}
