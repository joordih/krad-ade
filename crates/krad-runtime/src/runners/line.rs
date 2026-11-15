use krad_host::runner::connect_command;
use krad_host::{HostError, HostLink};
use krad_protocol::Hello;
use std::io::{self, BufRead as _, BufReader, Read};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

const REAPER_THREAD: &str = "krad-runtime-runner-line";
const STDERR_THREAD: &str = "krad-runtime-runner-line-stderr";
const NO_PIPES: &str = "the runner's connection has no pipes";
const REAP_POLL: Duration = Duration::from_millis(250);

#[derive(Debug)]
pub struct LineStop(Sender<()>);

impl LineStop {
    pub fn stop(&self) {
        if self.0.send(()).is_err() {
            tracing::debug!("the runner connection had already ended");
        }
    }
}

#[derive(Debug)]
pub struct RunnerLine {
    pub input: ChildStdout,
    pub output: ChildStdin,
    pub hello: Hello,
    pub stop: LineStop,
}

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum RunnerLineError {
    #[error("the cloud runner is not ready")]
    NotReady,
    #[error("could not reach the cloud runner's runtime: {0}")]
    Failed(String),
}

impl From<HostError> for RunnerLineError {
    fn from(error: HostError) -> Self {
        Self::Failed(error.to_string())
    }
}

impl From<io::Error> for RunnerLineError {
    fn from(error: io::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

fn ended(waited: io::Result<std::process::ExitStatus>) {
    match waited {
        Ok(status) => tracing::debug!(%status, "a runner connection ended"),
        Err(error) => tracing::debug!(%error, "a runner connection was not waited for"),
    }
}

fn watch(mut child: Child, stops: &Receiver<()>) {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return ended(Ok(status)),
            Ok(None) => {}
            Err(error) => return ended(Err(error)),
        }
        match stops.recv_timeout(REAP_POLL) {
            Ok(()) => {
                if let Err(error) = child.kill() {
                    tracing::debug!(%error, "a stopped runner connection could not be killed");
                }
                return ended(child.wait());
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return ended(child.wait()),
        }
    }
}

fn tell_what_it_says(stderr: impl Read + Send + 'static) -> io::Result<()> {
    thread::Builder::new()
        .name(STDERR_THREAD.to_owned())
        .spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let said = line.trim();
                if !said.is_empty() {
                    tracing::warn!(said, "the runner connection's ssh said");
                }
            }
        })
        .map(drop)
}

fn reap(child: Child) -> io::Result<LineStop> {
    let (stop, stops) = channel();
    thread::Builder::new()
        .name(REAPER_THREAD.to_owned())
        .spawn(move || watch(child, &stops))
        .map(|_| LineStop(stop))
}

#[tracing::instrument(skip_all)]
pub(crate) fn open_runner_line(
    link: &HostLink,
    cli: &Path,
    hello: Hello,
) -> Result<RunnerLine, RunnerLineError> {
    let mut command = connect_command(cli).build(link)?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    if let Some(stderr) = child.stderr.take() {
        tell_what_it_says(stderr)?;
    }
    let (Some(output), Some(input)) = (child.stdin.take(), child.stdout.take()) else {
        child.kill().ok();
        return Err(RunnerLineError::Failed(NO_PIPES.to_owned()));
    };
    let stop = reap(child)?;
    Ok(RunnerLine {
        input,
        output,
        hello,
        stop,
    })
}
