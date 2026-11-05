use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{HostError, Waited};

const STDERR_LIMIT_BYTES: u64 = 16 * 1024;
const READER_THREAD: &str = "krad-ade-host-output";

enum Stream {
    Out(Vec<u8>),
    Err(Vec<u8>),
}

/// # Errors
///
/// Returns [`HostError::TimedOut`] when the command is still running at `timeout`.
pub fn local_output_within(command: Command, timeout: Duration) -> Result<Output, HostError> {
    output_within(command, timeout, Waited::Command)
}

pub(crate) fn output_within(
    mut command: Command,
    timeout: Duration,
    waited: Waited,
) -> Result<Output, HostError> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let (sender, streams) = mpsc::channel();
    let mut readers = 0;
    if let Some(mut stdout) = child.stdout.take() {
        let sender = sender.clone();
        thread::Builder::new()
            .name(READER_THREAD.to_owned())
            .spawn(move || {
                let mut bytes = Vec::new();
                stdout.read_to_end(&mut bytes).ok();
                sender.send(Stream::Out(bytes)).ok();
            })?;
        readers += 1;
    }
    if let Some(stderr) = child.stderr.take() {
        let sender = sender.clone();
        thread::Builder::new()
            .name(READER_THREAD.to_owned())
            .spawn(move || {
                let mut bytes = Vec::new();
                let mut limited = stderr.take(STDERR_LIMIT_BYTES);
                limited.read_to_end(&mut bytes).ok();
                std::io::copy(&mut limited.into_inner(), &mut std::io::sink()).ok();
                sender.send(Stream::Err(bytes)).ok();
            })?;
        readers += 1;
    }
    drop(sender);
    let deadline = Instant::now() + timeout;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    for _ in 0..readers {
        let left = deadline.saturating_duration_since(Instant::now());
        match streams.recv_timeout(left) {
            Ok(Stream::Out(bytes)) => stdout = bytes,
            Ok(Stream::Err(bytes)) => stderr = bytes,
            Err(_) => {
                child.kill().ok();
                child.wait().ok();
                return Err(HostError::TimedOut(waited));
            }
        }
    }
    let status = child.wait()?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_command_that_hangs_is_stopped_at_its_deadline() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 30"]);
        let started = Instant::now();
        let result = output_within(command, Duration::from_millis(300), Waited::Command);
        assert!(matches!(result, Err(HostError::TimedOut(Waited::Command))));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn output_is_kept_and_stderr_is_bounded() -> Result<(), HostError> {
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "printf out; head -c 100000 /dev/zero | tr '\\0' e >&2; exit 4",
        ]);
        let output = output_within(command, Duration::from_secs(10), Waited::Command)?;
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr.len(), 16 * 1024);
        assert_eq!(output.status.code(), Some(4));
        Ok(())
    }
}
