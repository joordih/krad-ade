use super::*;
use crate::acp::files::{ReadTextFileRequest, WriteTextFileRequest};
use crate::acp::ids::SessionId;
use crate::acp::setup::EnvVariable;
use crate::acp::terminals::CreateTerminalRequest;
use krad_domain::LineChange;
use std::collections::BTreeMap;
use std::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
mod terminals;

type TestResult = Result<(), Box<dyn Error>>;

const WAIT: Duration = Duration::from_secs(10);
const SESSION: &str = "session-1";

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or_default(|elapsed| elapsed.as_nanos());
        let at = NEXT_SCRATCH.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "krad-acp-client-side-{}-{nanos}-{at}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap_or_default();
        Self(dir)
    }

    fn file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, content).unwrap_or_default();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap_or_default();
    }
}

#[derive(Default)]
struct FakeBuffers {
    unsaved: Mutex<BTreeMap<PathBuf, String>>,
    written: Mutex<Vec<(PathBuf, String)>>,
}

impl OpenBuffers for FakeBuffers {
    fn unsaved_text(&self, path: &Path) -> Option<String> {
        self.unsaved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .cloned()
    }

    fn written(&self, path: &Path, content: &str) {
        self.written
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((path.to_path_buf(), content.to_owned()));
    }
}

struct Files {
    scratch: Scratch,
    buffers: Arc<FakeBuffers>,
    host: HostFiles,
    written: Receiver<FileWritten>,
}

fn files() -> Files {
    let scratch = Scratch::new();
    let buffers = Arc::new(FakeBuffers::default());
    let (sender, written) = channel();
    let host = HostFiles::new(
        HostLink::Local,
        scratch.0.clone(),
        Arc::clone(&buffers) as Arc<dyn OpenBuffers>,
        sender,
    );
    Files {
        scratch,
        buffers,
        host,
        written,
    }
}

fn read(path: PathBuf, line: Option<u32>, limit: Option<u32>) -> ReadTextFileRequest {
    ReadTextFileRequest {
        session_id: SessionId(SESSION.to_owned()),
        path,
        line,
        limit,
    }
}

#[test]
fn a_file_is_read_from_disk_when_no_tab_holds_it() -> TestResult {
    let files = files();
    let path = files.scratch.file("notes.txt", "one\ntwo\n");
    let read = files.host.read(&read(path, None, None))?;
    assert_eq!(read.content, "one\ntwo\n");
    Ok(())
}

#[test]
fn an_unsaved_tab_wins_over_the_disk() -> TestResult {
    let files = files();
    let path = files.scratch.file("notes.txt", "saved\n");
    files
        .buffers
        .unsaved
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(path.clone(), "unsaved\n".to_owned());
    let read = files.host.read(&read(path, None, None))?;
    assert_eq!(read.content, "unsaved\n");
    Ok(())
}

#[test]
fn line_and_limit_pick_a_window_of_lines() -> TestResult {
    let files = files();
    let path = files.scratch.file("lines.txt", "1\n2\n3\n4\n5");
    let window = files.host.read(&read(path.clone(), Some(2), Some(2)))?;
    assert_eq!(window.content, "2\n3\n");
    let tail = files.host.read(&read(path.clone(), Some(4), None))?;
    assert_eq!(tail.content, "4\n5");
    let head = files.host.read(&read(path, None, Some(1)))?;
    assert_eq!(head.content, "1\n");
    Ok(())
}

#[test]
fn a_missing_file_is_reported_as_not_found() {
    let files = files();
    let missing = files.scratch.0.join("missing.txt");
    let refused = files.host.read(&read(missing, None, None)).err();
    assert_eq!(
        refused.map(|error| error.code),
        Some(ErrorCode::RESOURCE_NOT_FOUND)
    );
}

#[test]
fn a_write_lands_on_disk_reports_its_edit_and_tells_the_tab() -> TestResult {
    let files = files();
    let path = files.scratch.file("a.txt", "old\n");
    files.host.write(WriteTextFileRequest {
        session_id: SessionId(SESSION.to_owned()),
        path: path.clone(),
        content: "new\n".to_owned(),
    })?;
    assert_eq!(std::fs::read_to_string(&path)?, "new\n");
    let written = files.written.recv_timeout(WAIT)?;
    assert_eq!(written.session_id.0, SESSION);
    assert_eq!(written.path, path);
    assert_eq!(written.edit.path, "a.txt");
    assert_eq!(written.edit.counts(), (1, 1));
    assert!(written
        .edit
        .lines
        .iter()
        .any(|line| line.change == LineChange::Removed && line.text == "old"));
    let told = files
        .buffers
        .written
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert_eq!(told, [(path, "new\n".to_owned())]);
    Ok(())
}

#[test]
fn a_write_creates_missing_folders() -> TestResult {
    let files = files();
    let path = files.scratch.0.join("deep/down/new.txt");
    files.host.write(WriteTextFileRequest {
        session_id: SessionId(SESSION.to_owned()),
        path: path.clone(),
        content: "hello\n".to_owned(),
    })?;
    assert_eq!(std::fs::read_to_string(&path)?, "hello\n");
    assert_eq!(files.written.recv_timeout(WAIT)?.edit.counts(), (1, 0));
    Ok(())
}

#[test]
fn relative_paths_are_invalid_params() {
    let files = files();
    let relative = PathBuf::from("src/main.rs");
    let read = files.host.read(&read(relative.clone(), None, None)).err();
    let write = files
        .host
        .write(WriteTextFileRequest {
            session_id: SessionId(SESSION.to_owned()),
            path: relative,
            content: String::new(),
        })
        .err();
    assert_eq!(
        read.map(|error| error.code),
        Some(ErrorCode::INVALID_PARAMS)
    );
    assert_eq!(
        write.map(|error| error.code),
        Some(ErrorCode::INVALID_PARAMS)
    );
    assert!(files.written.try_recv().is_err());
}

fn create(command: &str, cwd: Option<PathBuf>, limit: Option<u64>) -> CreateTerminalRequest {
    CreateTerminalRequest {
        session_id: SessionId(SESSION.to_owned()),
        command: "sh".to_owned(),
        args: vec!["-c".to_owned(), command.to_owned()],
        env: vec![EnvVariable {
            name: "KRAD_ACP_VALUE".to_owned(),
            value: "from-env".to_owned(),
        }],
        cwd,
        output_byte_limit: limit,
    }
}

#[test]
fn a_relative_terminal_folder_is_invalid_params() {
    let scratch = Scratch::new();
    let terminals = HostTerminals::new(HostLink::Local, scratch.0.clone());
    let refused = terminals
        .create(&create("true", Some(PathBuf::from("relative")), None))
        .err();
    assert_eq!(
        refused.map(|error| error.code),
        Some(ErrorCode::INVALID_PARAMS)
    );
}
