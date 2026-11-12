use std::io::{self, Write};
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};

use super::{harness_command, Runner};
use crate::cloud::HarnessLines;

pub(crate) struct Harness {
    pub(crate) child: Child,
    pub(crate) stdin: Option<ChildStdin>,
    pub(crate) stdout: Option<HarnessLines<ChildStdout>>,
    pub(crate) stderr: Option<ChildStderr>,
}

fn hand_over(child: &mut Child, stdin: Option<&mut ChildStdin>, input: &str) -> io::Result<()> {
    let Some(stdin) = stdin else {
        return Ok(());
    };
    let written = stdin
        .write_all(input.as_bytes())
        .and_then(|()| stdin.flush());
    if written.is_err() {
        child.kill().unwrap_or_default();
        drop(child.wait());
    }
    written
}

fn start(
    mut command: Command,
    input: Option<&str>,
    lines: impl FnOnce(ChildStdout) -> HarnessLines<ChildStdout>,
) -> io::Result<Harness> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take();
    if let Some(input) = input {
        hand_over(&mut child, stdin.as_mut(), input)?;
    }
    Ok(Harness {
        stdin,
        stdout: child.stdout.take().map(lines),
        stderr: child.stderr.take(),
        child,
    })
}

pub(crate) fn spawn_fed(command: Command, input: &str) -> io::Result<Harness> {
    start(command, Some(input), HarnessLines::plain)
}

pub(crate) fn spawn_harness(
    runner: &Runner,
    cwd: &Path,
    args: &[String],
    env: &[(String, String)],
) -> io::Result<Harness> {
    let Runner::Cloud(cloud) = runner else {
        return start(
            harness_command(runner, cwd, args, env)?,
            None,
            HarnessLines::plain,
        );
    };
    let secret = cloud.command(cwd, args, env)?;
    let cursor = cloud.cursor();
    start(secret.command, secret.input.as_deref(), |stdout| {
        HarnessLines::cloud(stdout, cursor)
    })
}
