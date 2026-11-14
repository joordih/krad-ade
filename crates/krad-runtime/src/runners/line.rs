use krad_host::runner::connect_command;
use krad_host::{HostError, HostLink};
use krad_protocol::Hello;
use std::io;
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::thread;

const REAPER_THREAD: &str = "krad-runtime-runner-line";
const NO_PIPES: &str = "the runner's connection has no pipes";

#[derive(Debug)]
pub struct RunnerLine {
    pub input: ChildStdout,
    pub output: ChildStdin,
    pub hello: Hello,
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

fn reap(mut child: Child) -> io::Result<()> {
    thread::Builder::new()
        .name(REAPER_THREAD.to_owned())
        .spawn(move || match child.wait() {
            Ok(status) => tracing::debug!(%status, "a runner connection ended"),
            Err(error) => tracing::debug!(%error, "a runner connection was not waited for"),
        })
        .map(drop)
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
        .stderr(Stdio::null());
    let mut child = command.spawn()?;
    let (Some(output), Some(input)) = (child.stdin.take(), child.stdout.take()) else {
        child.kill().ok();
        return Err(RunnerLineError::Failed(NO_PIPES.to_owned()));
    };
    reap(child)?;
    Ok(RunnerLine {
        input,
        output,
        hello,
    })
}
