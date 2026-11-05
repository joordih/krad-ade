use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::super::bootstrap::{self, Greeting};
use super::super::session::{Closer, Incoming, Session, Side};
use super::super::MuxError;
use crate::error::{HostError, Waited};
use crate::failure::SshFailure;
use crate::layout::APP_VERSION;
use crate::platform::{HostPlatform, UnsupportedPlatform};
use crate::remote_link::lock;

const SIGN_IN_WAIT: Duration = Duration::from_mins(5);
const STDERR_LIMIT_BYTES: u64 = 16 * 1024;
const BOOT_THREAD: &str = "krad-ade-mux-boot";
const ERRORS_THREAD: &str = "krad-ade-mux-ssh-errors";
const STDERR_SETTLE: Duration = Duration::from_millis(100);
const EXIT_POLL: Duration = Duration::from_millis(20);
const HOST_KEY_NOT_TRUSTED: &str = "the host's key was not trusted";
const CHANNEL_CLOSED: &str = "it ended before the app's CLI started";
const HEADLINE_MAX_CHARS: usize = 200;

type Booted = Result<(Session, mpsc::Receiver<Incoming>, Greeting), MuxError>;

pub(super) fn boot(
    mut stdin: ChildStdin,
    stdout: ChildStdout,
    host_clis: Vec<(HostPlatform, PathBuf)>,
    child: &Arc<Mutex<Child>>,
    handshake: Duration,
) -> Booted {
    let (sender, booted) = mpsc::channel();
    let killer = Arc::clone(child);
    thread::Builder::new()
        .name(BOOT_THREAD.to_owned())
        .spawn(move || {
            let result = (|| {
                let mut reader = BufReader::new(stdout);
                let greeting = bootstrap::read_greeting(&mut reader)?;
                bootstrap::answer(&mut stdin, &greeting, &host_clis)?;
                let closer: Closer = Box::new(move || stop(&killer));
                let (session, incoming, _) = Session::start(
                    reader,
                    stdin,
                    Side::Client,
                    APP_VERSION,
                    handshake,
                    Some(closer),
                )?;
                Ok((session, incoming, greeting))
            })();
            sender.send(result).ok();
        })
        .map_err(MuxError::Io)?;
    booted
        .recv_timeout(SIGN_IN_WAIT)
        .map_err(|_| MuxError::TimedOut)?
}

pub(super) fn exited_within(child: &Mutex<Child>, wait: Duration) -> Option<i32> {
    let deadline = std::time::Instant::now() + wait;
    loop {
        if let Ok(Some(status)) = lock(child).try_wait() {
            return status.code();
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        thread::sleep(EXIT_POLL);
    }
}

pub(super) fn stop(child: &Mutex<Child>) {
    let mut child = lock(child);
    if child.kill().is_ok() {
        child.wait().ok();
    }
}

pub(super) fn collect_errors(child: &mut Child) -> Result<Arc<Mutex<Vec<u8>>>, HostError> {
    let collected = Arc::new(Mutex::new(Vec::new()));
    if let Some(stderr) = child.stderr.take() {
        let sink = Arc::clone(&collected);
        thread::Builder::new()
            .name(ERRORS_THREAD.to_owned())
            .spawn(move || {
                let mut bytes = Vec::new();
                let mut limited = stderr.take(STDERR_LIMIT_BYTES);
                limited.read_to_end(&mut bytes).ok();
                *lock(&sink) = bytes;
                std::io::copy(&mut limited.into_inner(), &mut std::io::sink()).ok();
            })?;
    }
    Ok(collected)
}

pub(super) fn boot_failure(error: MuxError, errors: &Mutex<Vec<u8>>) -> HostError {
    match error {
        MuxError::Closed | MuxError::Io(_) | MuxError::Protocol => {
            thread::sleep(STDERR_SETTLE);
            let stderr = String::from_utf8_lossy(&lock(errors)).into_owned();
            let failure = match SshFailure::classify(&stderr) {
                SshFailure::NeedsInteractiveSignIn => SshFailure::AuthenticationFailed,
                SshFailure::HostKeyUnknown => SshFailure::Other(HOST_KEY_NOT_TRUSTED.to_owned()),
                failure => failure,
            };
            tracing::debug!(%failure, "the multiplexed session did not start");
            HostError::Ssh(failure)
        }
        MuxError::TimedOut => HostError::TimedOut(Waited::Connect),
        MuxError::Unsupported(uname) => HostError::UnsupportedPlatform(UnsupportedPlatform(uname)),
        other => HostError::Mux(other),
    }
}

pub(super) fn machine_failure(error: MuxError, errors: &Mutex<Vec<u8>>) -> HostError {
    match error {
        MuxError::Closed | MuxError::Io(_) | MuxError::Protocol => {
            thread::sleep(STDERR_SETTLE);
            let stderr = String::from_utf8_lossy(&lock(errors)).into_owned();
            let headline: String = stderr
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or(CHANNEL_CLOSED)
                .chars()
                .take(HEADLINE_MAX_CHARS)
                .collect();
            tracing::debug!("the channel to the machine did not start");
            HostError::MachineChannel(headline)
        }
        MuxError::TimedOut => HostError::TimedOut(Waited::Connect),
        MuxError::Unsupported(uname) => HostError::UnsupportedPlatform(UnsupportedPlatform(uname)),
        other => HostError::Mux(other),
    }
}
