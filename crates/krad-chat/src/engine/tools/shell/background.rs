use std::collections::{BTreeMap, VecDeque};
use std::io::Read;
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use super::{clip, platform, CHUNK_BYTES, NOT_STARTED_TEXT, NO_OUTPUT_TEXT};
use crate::engine::tools::{Ran, Workspace};

const MAX_KEPT_BYTES: usize = 256 * 1024;
const MAX_PROCESSES: usize = 16;
const ID_PREFIX: &str = "bg";
const RUNNING_TEXT: &str = "[still running]";
const SIGNAL_TEXT: &str = "[exited after a signal]";
const STOPPED_TEXT: &str = "[stopped]";
const TOO_MANY_TEXT: &str = "Too many background shells are open; stop one with shell_stop first";
const UNKNOWN_TEXT: &str = "No background shell has the id";
const STARTED_TAIL: &str =
    "Read its new output with shell_output and stop it with shell_stop when done.";

#[derive(Default)]
struct Output {
    kept: VecDeque<u8>,
    start: u64,
    read: u64,
}

impl Output {
    fn push(&mut self, bytes: &[u8]) {
        self.kept.extend(bytes);
        let over = self.kept.len().saturating_sub(MAX_KEPT_BYTES);
        self.kept.drain(..over);
        self.start += over as u64;
    }

    fn take_new(&mut self) -> (u64, Vec<u8>) {
        let from = self.read.max(self.start);
        let skipped = from - self.read;
        let offset = usize::try_from(from - self.start).unwrap_or(self.kept.len());
        let new: Vec<u8> = self.kept.iter().skip(offset).copied().collect();
        self.read = self.start + self.kept.len() as u64;
        (skipped, new)
    }
}

type Shared = Arc<Mutex<Output>>;

struct Process {
    child: Child,
    output: Shared,
}

impl Process {
    fn stop(&mut self) {
        platform::kill(&mut self.child);
        self.child.wait().map_or_default(drop);
    }
}

#[derive(Default)]
struct Registry {
    next: u32,
    processes: BTreeMap<String, Process>,
}

impl Registry {
    fn new_id(&mut self) -> String {
        self.next += 1;
        format!("{ID_PREFIX}{}", self.next)
    }

    fn stop_all(&mut self) {
        for process in self.processes.values_mut() {
            process.stop();
        }
        self.processes.clear();
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        self.stop_all();
    }
}

#[derive(Clone, Default)]
pub(crate) struct Background(Arc<Mutex<Registry>>);

fn pump(mut stream: impl Read, output: &Shared) {
    let mut buffer = [0; CHUNK_BYTES];
    while let Ok(read) = stream.read(&mut buffer) {
        if read == 0 {
            break;
        }
        output
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(&buffer[..read]);
    }
}

fn listen(stream: Option<impl Read + Send + 'static>, output: &Shared) {
    let Some(stream) = stream else {
        return;
    };
    let output = Arc::clone(output);
    drop(thread::spawn(move || pump(stream, &output)));
}

fn report(output: &Shared, status: &str) -> String {
    let (skipped, new) = output
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take_new();
    let text = String::from_utf8_lossy(&new);
    let text = text.trim_end();
    let mut content = if skipped > 0 {
        format!("[{skipped} earlier bytes dropped]\n")
    } else {
        String::new()
    };
    content.push_str(if text.is_empty() {
        NO_OUTPUT_TEXT
    } else {
        text
    });
    let content = clip(&content);
    format!("{content}\n{status}")
}

impl Background {
    fn registry(&self) -> std::sync::MutexGuard<'_, Registry> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn start(&self, workspace: &Workspace, line: &str) -> Ran {
        let mut registry = self.registry();
        if registry.processes.len() >= MAX_PROCESSES {
            return Ran::failed(TOO_MANY_TEXT.to_owned());
        }
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
        let output = Shared::default();
        listen(child.stdout.take(), &output);
        listen(child.stderr.take(), &output);
        let id = registry.new_id();
        registry
            .processes
            .insert(id.clone(), Process { child, output });
        Ran::done(format!(
            "Started in the background with id {id}. {STARTED_TAIL}"
        ))
    }

    pub(crate) fn output(&self, id: &str) -> Ran {
        let mut registry = self.registry();
        let Some(process) = registry.processes.get_mut(id) else {
            return Ran::failed(format!("{UNKNOWN_TEXT} {id}"));
        };
        let status = match process.child.try_wait() {
            Ok(None) => RUNNING_TEXT.to_owned(),
            Ok(Some(status)) => status.code().map_or_else(
                || SIGNAL_TEXT.to_owned(),
                |code| format!("[exited with code {code}]"),
            ),
            Err(error) => format!("[{error}]"),
        };
        Ran::done(report(&process.output, &status))
    }

    pub(crate) fn stop(&self, id: &str) -> Ran {
        let Some(mut process) = self.registry().processes.remove(id) else {
            return Ran::failed(format!("{UNKNOWN_TEXT} {id}"));
        };
        process.stop();
        Ran::done(report(&process.output, STOPPED_TEXT))
    }

    pub(crate) fn stop_all(&self) {
        self.registry().stop_all();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::super::scratch::Scratch;
    use super::*;
    use std::time::{Duration, Instant};

    const WAIT: Duration = Duration::from_secs(10);
    const POLL: Duration = Duration::from_millis(50);

    fn workspace(scratch: &Scratch) -> Workspace {
        Workspace {
            cwd: scratch.path().to_path_buf(),
            env: vec![("KRAD_BG_VALUE".to_owned(), "from-env".to_owned())],
        }
    }

    fn until_output(background: &Background, id: &str, wanted: &str) -> String {
        let started = Instant::now();
        let mut seen = String::new();
        while started.elapsed() < WAIT {
            seen.push_str(&background.output(id).content);
            if seen.contains(wanted) {
                break;
            }
            thread::sleep(POLL);
        }
        seen
    }

    #[test]
    fn a_background_shell_reports_new_output_then_its_exit() {
        let scratch = Scratch::new();
        let background = Background::default();
        let started = background.start(
            &workspace(&scratch),
            "echo $KRAD_BG_VALUE; sleep 0.3; echo second",
        );
        assert!(started.content.contains("id bg1"), "{}", started.content);
        let first = until_output(&background, "bg1", "from-env");
        assert!(first.contains("from-env"), "{first}");
        let rest = until_output(&background, "bg1", "[exited with code 0]");
        assert!(
            rest.contains("second") && !rest.contains("from-env"),
            "{rest}"
        );
        assert!(background.output("bg9").failed);
    }

    #[test]
    fn stopping_kills_the_process_and_forgets_it() {
        let scratch = Scratch::new();
        let background = Background::default();
        background.start(&workspace(&scratch), "sleep 30");
        let started = Instant::now();
        let stopped = background.stop("bg1");
        assert!(
            stopped.content.ends_with(STOPPED_TEXT),
            "{}",
            stopped.content
        );
        assert!(started.elapsed() < WAIT);
        assert!(background.output("bg1").failed);
        background.start(&workspace(&scratch), "sleep 30");
        background.stop_all();
        assert!(background.output("bg2").failed);
    }

    #[test]
    fn kept_output_is_capped_and_the_drop_is_reported() {
        let mut output = Output::default();
        output.push(&vec![b'a'; MAX_KEPT_BYTES]);
        output.push(b"tail");
        let (skipped, new) = output.take_new();
        assert_eq!(skipped, 4);
        assert!(new.ends_with(b"tail"));
        assert_eq!(output.take_new(), (0, Vec::new()));
    }
}
