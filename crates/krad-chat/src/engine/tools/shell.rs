mod background;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
use windows as platform;

use std::fmt::Write as _;
use std::io::Read;
use std::process::{ChildStderr, ChildStdout, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::{Ran, Workspace};
pub(crate) use background::Background;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_TIMEOUT: Duration = Duration::from_secs(600);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const DRAIN_GRACE: Duration = Duration::from_secs(2);
const MAX_OUTPUT_CHARS: usize = 30_000;
const CHUNK_BYTES: usize = 8192;
const STREAMS: usize = 2;
const NO_OUTPUT_TEXT: &str = "(no output)";
const CANCELLED_TEXT: &str = "[cancelled]";
const SIGNAL_TEXT: &str = "[stopped by a signal]";
const NOT_STARTED_TEXT: &str = "The command could not start";

enum Piece {
    Bytes(Vec<u8>),
    Closed,
}

enum Ending {
    Exited(Option<i32>),
    TimedOut(Duration),
    Cancelled,
}

fn pump(mut stream: impl Read, pieces: &Sender<Piece>) {
    let mut buffer = [0; CHUNK_BYTES];
    while let Ok(read) = stream.read(&mut buffer) {
        if read == 0 || pieces.send(Piece::Bytes(buffer[..read].to_vec())).is_err() {
            break;
        }
    }
    pieces.send(Piece::Closed).unwrap_or_default();
}

fn listen(stdout: Option<ChildStdout>, stderr: Option<ChildStderr>) -> Receiver<Piece> {
    let (pieces, received) = channel();
    let other = pieces.clone();
    match stdout {
        Some(stdout) => drop(thread::spawn(move || pump(stdout, &pieces))),
        None => pieces.send(Piece::Closed).unwrap_or_default(),
    }
    match stderr {
        Some(stderr) => drop(thread::spawn(move || pump(stderr, &other))),
        None => other.send(Piece::Closed).unwrap_or_default(),
    }
    received
}

fn collect(received: &Receiver<Piece>, output: &mut Vec<u8>, closed: &mut usize) {
    while let Ok(piece) = received.try_recv() {
        match piece {
            Piece::Bytes(bytes) => output.extend(bytes),
            Piece::Closed => *closed += 1,
        }
    }
}

fn drain(received: &Receiver<Piece>, output: &mut Vec<u8>, mut closed: usize) {
    let deadline = Instant::now() + DRAIN_GRACE;
    while closed < STREAMS {
        let left = deadline.saturating_duration_since(Instant::now());
        match received.recv_timeout(left) {
            Ok(Piece::Bytes(bytes)) => output.extend(bytes),
            Ok(Piece::Closed) => closed += 1,
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn clip(text: &str) -> String {
    let count = text.chars().count();
    if count <= MAX_OUTPUT_CHARS {
        return text.to_owned();
    }
    let half = MAX_OUTPUT_CHARS / 2;
    let head: String = text.chars().take(half).collect();
    let tail: String = text.chars().skip(count - half).collect();
    format!(
        "{head}\n… [{} characters cut] …\n{tail}",
        count - MAX_OUTPUT_CHARS
    )
}

fn report(output: &[u8], ending: &Ending) -> Ran {
    let text = String::from_utf8_lossy(output);
    let text = text.trim_end();
    let mut content = if text.is_empty() {
        NO_OUTPUT_TEXT.to_owned()
    } else {
        clip(text)
    };
    let failed = match ending {
        Ending::Exited(Some(0)) => false,
        Ending::Exited(Some(code)) => {
            write!(content, "\n[exit code {code}]").unwrap_or_default();
            true
        }
        Ending::Exited(None) => {
            content.push('\n');
            content.push_str(SIGNAL_TEXT);
            true
        }
        Ending::TimedOut(after) => {
            write!(content, "\n[timed out after {} s]", after.as_secs()).unwrap_or_default();
            true
        }
        Ending::Cancelled => {
            content.push('\n');
            content.push_str(CANCELLED_TEXT);
            true
        }
    };
    Ran {
        content,
        failed,
        edit: None,
    }
}

pub(super) fn run(
    workspace: &Workspace,
    line: &str,
    timeout: Option<Duration>,
    cancel: &AtomicBool,
) -> Ran {
    let timeout = timeout.unwrap_or(DEFAULT_TIMEOUT).min(MAX_TIMEOUT);
    let mut command = platform::command(line);
    command
        .current_dir(&workspace.cwd)
        .envs(workspace.env.iter().map(|(name, value)| (name, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return Ran::failed(format!("{NOT_STARTED_TEXT}: {error}")),
    };
    let received = listen(child.stdout.take(), child.stderr.take());
    let started = Instant::now();
    let mut output = Vec::new();
    let mut closed = 0;
    let ending = loop {
        collect(&received, &mut output, &mut closed);
        match child.try_wait() {
            Ok(Some(status)) => break Ending::Exited(status.code()),
            Ok(None) => {}
            Err(_) => break Ending::Exited(None),
        }
        if cancel.load(Ordering::SeqCst) {
            platform::kill(&mut child);
            break Ending::Cancelled;
        }
        if started.elapsed() >= timeout {
            platform::kill(&mut child);
            break Ending::TimedOut(timeout);
        }
        thread::sleep(POLL_INTERVAL);
    };
    child.wait().map_or_default(drop);
    drain(&received, &mut output, closed);
    report(&output, &ending)
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::scratch::Scratch;
    use super::*;
    use std::sync::Arc;

    fn workspace(scratch: &Scratch) -> Workspace {
        Workspace {
            cwd: scratch.path().to_path_buf(),
            env: vec![("KRAD_TEST_VALUE".to_owned(), "from-env".to_owned())],
        }
    }

    #[test]
    fn a_command_runs_in_the_workspace_with_its_env() {
        let scratch = Scratch::new();
        scratch.file("marker.txt", "");
        let cancel = AtomicBool::new(false);
        let ran = run(
            &workspace(&scratch),
            "ls && echo $KRAD_TEST_VALUE",
            None,
            &cancel,
        );
        assert!(!ran.failed, "{}", ran.content);
        assert!(ran.content.contains("marker.txt"));
        assert!(ran.content.ends_with("from-env"));
    }

    #[test]
    fn a_failing_command_reports_its_exit_code_and_stderr() {
        let scratch = Scratch::new();
        let cancel = AtomicBool::new(false);
        let ran = run(&workspace(&scratch), "echo oops >&2; exit 3", None, &cancel);
        assert!(ran.failed);
        assert_eq!(ran.content, "oops\n[exit code 3]");
    }

    #[test]
    fn a_slow_command_stops_at_its_timeout() {
        let scratch = Scratch::new();
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        let ran = run(
            &workspace(&scratch),
            "sleep 30",
            Some(Duration::from_secs(1)),
            &cancel,
        );
        assert!(ran.failed && ran.content.contains("timed out after 1 s"));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_cancelled_command_is_killed() {
        let scratch = Scratch::new();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let stopper = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            flag.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let ran = run(&workspace(&scratch), "sleep 30", None, &cancel);
        stopper.join().unwrap_or_default();
        assert!(ran.failed && ran.content.ends_with(CANCELLED_TEXT));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn long_output_keeps_its_head_and_tail() {
        let long = format!("start{}end", "x".repeat(MAX_OUTPUT_CHARS * 2));
        let clipped = clip(&long);
        assert!(clipped.starts_with("start") && clipped.ends_with("end"));
        assert!(clipped.chars().count() < long.chars().count());
    }
}
