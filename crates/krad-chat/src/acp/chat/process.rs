use std::io;
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use tracing::{debug, info};

use crate::cloud::HarnessLines;
use crate::live::keep_stderr_tail;
use crate::process::{spawn_harness, Harness, Runner};

const NO_PIPE_TEXT: &str = "the agent started without its standard pipes";
const STOPPED_BEFORE_START_TEXT: &str = "the chat stopped before the agent started";

#[derive(Debug, Default)]
struct Slot {
    child: Option<Child>,
    stopped: bool,
}

#[derive(Debug, Default)]
pub(super) struct AgentProcess {
    slot: Mutex<Slot>,
    stderr_tail: Arc<Mutex<String>>,
}

pub(super) struct Pipes {
    pub(super) stdout: HarnessLines<ChildStdout>,
    pub(super) stdin: ChildStdin,
}

fn missing_pipe() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, NO_PIPE_TEXT)
}

impl AgentProcess {
    #[tracing::instrument(skip_all, fields(wsl = runner.in_wsl(), ssh = runner.on_ssh()))]
    pub(super) fn launch(
        &self,
        runner: &Runner,
        cwd: &Path,
        args: &[String],
        env: &[(String, String)],
    ) -> io::Result<Pipes> {
        let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.stopped {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                STOPPED_BEFORE_START_TEXT,
            ));
        }
        let Harness {
            child,
            stdin,
            stdout,
            stderr,
        } = spawn_harness(runner, cwd, args, env)?;
        if let Some(stderr) = stderr {
            let tail = Arc::clone(&self.stderr_tail);
            thread::spawn(move || keep_stderr_tail(stderr, &tail));
        }
        let pipes = stdout
            .zip(stdin)
            .map(|(stdout, stdin)| Pipes { stdout, stdin });
        info!(pid = child.id(), "ACP agent started");
        slot.child = Some(child);
        drop(slot);
        pipes.ok_or_else(missing_pipe)
    }

    pub(super) fn stderr(&self) -> String {
        self.stderr_tail
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .trim()
            .to_owned()
    }

    pub(super) fn stop(&self) {
        let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        slot.stopped = true;
        let Some(mut child) = slot.child.take() else {
            return;
        };
        drop(slot);
        if let Err(error) = child.kill() {
            debug!(%error, "the ACP agent had already ended");
        }
        if let Err(error) = child.wait() {
            debug!(%error, "could not reap the ACP agent");
        }
        info!("ACP agent stopped");
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        self.stop();
    }
}
