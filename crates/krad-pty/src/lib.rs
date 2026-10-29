use krad_domain::INHERITED_SESSION_VARS;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use thiserror::Error;

#[cfg(not(windows))]
mod unix;
#[cfg(windows)]
mod windows;
mod wsl;

#[cfg(not(windows))]
pub use unix::{clear_screen_prefix, default_shell, log_spammer, login_shell_env, SHELL_QUOTING};
#[cfg(not(windows))]
use unix::{executable_names, is_executable};
#[cfg(windows)]
pub use windows::{
    clear_screen_prefix, default_shell, log_spammer, login_shell_env, SHELL_QUOTING,
};
#[cfg(windows)]
use windows::{executable_names, is_executable};
pub use wsl::{wsl_shell, wsl_shell_in};

const PATH_ENV: &str = "PATH";

#[must_use]
pub fn find_program(name: &str) -> Option<PathBuf> {
    let search = std::env::var_os(PATH_ENV)?;
    let names = executable_names(name);
    std::env::split_paths(&search)
        .flat_map(|dir| names.iter().map(move |file| dir.join(file)))
        .find(|candidate| is_executable(candidate))
}

pub const DEFAULT_COLS: u16 = 80;
pub const DEFAULT_ROWS: u16 = 24;
pub const READ_CHUNK: usize = 8192;
const EXIT_THREAD_NAME: &str = "krad-pty-exit";
const TERM_ENV: &str = "TERM";
const TERM_VALUE: &str = "xterm-256color";
const COLORTERM_ENV: &str = "COLORTERM";
const COLORTERM_VALUE: &str = "truecolor";

pub type PlatformError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Error)]
pub enum PtyError {
    #[error("cannot spawn pty child: {0}")]
    Spawn(#[source] PlatformError),
    #[error("pty I/O failure: {0}")]
    Io(#[from] std::io::Error),
}

impl PtyError {
    fn platform(error: impl Into<PlatformError>) -> Self {
        Self::Spawn(error.into())
    }
}

pub struct SpawnRequest {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
}

impl SpawnRequest {
    #[must_use]
    pub fn new(program: String) -> Self {
        Self {
            program,
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
        }
    }

    #[must_use]
    pub fn shell() -> Self {
        let (program, args) = default_shell();
        Self {
            program,
            args,
            ..Self::new(String::new())
        }
    }
}

pub struct SpawnedPty {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub master: SharedMaster,
    pub killer: SharedKiller,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub pid: Option<u32>,
}

#[derive(Clone)]
pub struct SharedMaster {
    inner: Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>,
}

impl SharedMaster {
    fn new(master: Box<dyn MasterPty + Send>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(master))),
        }
    }

    /// # Errors
    pub fn resize(&self, size: GridSize) -> Result<(), PtyError> {
        match self.inner.lock() {
            Ok(master) => master
                .as_ref()
                .map_or(Ok(()), |master| resize_master(master.as_ref(), size)),
            Err(_) => Ok(()),
        }
    }

    pub fn close(&self) {
        if let Ok(mut master) = self.inner.lock() {
            master.take();
        }
    }
}

pub struct SharedKiller {
    inner: std::sync::Mutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
}

impl SharedKiller {
    #[must_use]
    pub fn new(killer: Box<dyn portable_pty::ChildKiller + Send + Sync>) -> Self {
        Self {
            inner: std::sync::Mutex::new(killer),
        }
    }

    pub fn kill(&self) {
        if let Ok(killer) = self.inner.lock() {
            let mut killer = killer;
            drop(killer.kill());
        }
    }
}

/// # Errors
pub fn spawn_pty(request: &SpawnRequest) -> Result<SpawnedPty, PtyError> {
    let system = native_pty_system();
    let pair = system
        .openpty(PtySize {
            rows: request.rows,
            cols: request.cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(PtyError::platform)?;
    let mut cmd = CommandBuilder::new(&request.program);
    cmd.args(&request.args);
    if let Some(cwd) = &request.cwd {
        cmd.cwd(cwd);
    }
    for name in INHERITED_SESSION_VARS {
        cmd.env_remove(name);
    }
    cmd.env(TERM_ENV, TERM_VALUE);
    cmd.env(COLORTERM_ENV, COLORTERM_VALUE);
    for (key, value) in &request.env {
        cmd.env(key, value);
    }
    let child = pair.slave.spawn_command(cmd).map_err(PtyError::platform)?;
    let pid = child.process_id();
    let killer = SharedKiller::new(child.clone_killer());
    let reader = pair.master.try_clone_reader().map_err(PtyError::platform)?;
    let writer = pair.master.take_writer().map_err(PtyError::platform)?;
    Ok(SpawnedPty {
        reader,
        writer,
        master: SharedMaster::new(pair.master),
        killer,
        child,
        pid,
    })
}

impl SpawnedPty {
    /// # Errors
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        self.master.resize(GridSize {
            cols,
            rows,
            ..GridSize::default()
        })
    }
}

/// # Errors
pub fn read_to_end(reader: &mut Box<dyn Read + Send>) -> Result<Vec<u8>, PtyError> {
    let mut out = Vec::new();
    let mut chunk = vec![0u8; READ_CHUNK];
    loop {
        match reader.read(&mut chunk) {
            Ok(n) if n > 0 => out.extend_from_slice(&chunk[..n]),
            _ => return Ok(out),
        }
    }
}

/// # Errors
pub fn watch_exit(
    mut child: Box<dyn Child + Send + Sync>,
    master: SharedMaster,
) -> Result<JoinHandle<Option<i32>>, PtyError> {
    let handle = std::thread::Builder::new()
        .name(EXIT_THREAD_NAME.to_owned())
        .spawn(move || {
            let status = child.wait().ok();
            master.close();
            status.and_then(|status| i32::try_from(status.exit_code()).ok())
        })?;
    Ok(handle)
}

pub fn run_reader_loop(
    reader: &mut Box<dyn Read + Send>,
    cancelled: &AtomicBool,
    mut on_output: impl FnMut(&[u8]) -> bool,
) {
    let mut chunk = vec![0u8; READ_CHUNK];
    loop {
        if cancelled.load(Ordering::SeqCst) {
            break;
        }
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if !on_output(&chunk[..n]) {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => (),
            Err(_) => break,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct GridSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_width_px: u16,
    pub cell_height_px: u16,
}

impl GridSize {
    const fn pixels(self) -> (u16, u16) {
        (
            self.cols.saturating_mul(self.cell_width_px),
            self.rows.saturating_mul(self.cell_height_px),
        )
    }
}

#[derive(Debug)]
pub enum PtyInput {
    Bytes(Vec<u8>),
    Resize(GridSize),
}

/// # Errors
pub fn resize_master(master: &(dyn MasterPty + Send), size: GridSize) -> Result<(), PtyError> {
    master
        .resize({
            let (pixel_width, pixel_height) = size.pixels();
            PtySize {
                rows: size.rows,
                cols: size.cols,
                pixel_width,
                pixel_height,
            }
        })
        .map_err(PtyError::platform)
}

pub fn run_writer_loop(
    writer: &mut Box<dyn Write + Send>,
    inbox: Receiver<PtyInput>,
    mut on_resize: impl FnMut(GridSize),
) {
    for input in inbox {
        match input {
            PtyInput::Bytes(bytes) => {
                if writer.write_all(&bytes).is_err() {
                    break;
                }
            }
            PtyInput::Resize(size) => on_resize(size),
        }
    }
    drop(writer.flush());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn echo_request(marker: &str) -> SpawnRequest {
        let mut request = SpawnRequest::new("/bin/sh".to_owned());
        request.args = vec!["-c".to_owned(), format!("echo {marker}")];
        request
    }

    #[cfg(windows)]
    fn echo_request(marker: &str) -> SpawnRequest {
        let mut request = SpawnRequest::new("cmd.exe".to_owned());
        request.args = vec!["/C".to_owned(), format!("echo {marker}")];
        request
    }

    const CURSOR_QUERY: &[u8] = b"[6n";
    const CURSOR_REPLY: &[u8] = b"[1;1R";

    fn answer_queries(bytes: &[u8], writer: &mut Box<dyn Write + Send>) {
        if bytes
            .windows(CURSOR_QUERY.len())
            .any(|window| window == CURSOR_QUERY)
        {
            writer.write_all(CURSOR_REPLY).unwrap_or_default();
            writer.flush().unwrap_or_default();
        }
    }

    #[test]
    fn child_output_reaches_eof() {
        let request = echo_request("hello-pty-marker");
        let spawned = spawn_pty(&request);
        assert!(spawned.is_ok(), "pty spawns in this environment");
        let Ok(spawned) = spawned else {
            return;
        };
        let mut reader = spawned.reader;
        let mut writer = spawned.writer;
        let watcher = watch_exit(spawned.child, spawned.master);
        assert!(watcher.is_ok(), "the exit watcher starts");
        let mut bytes = Vec::new();
        let mut chunk = vec![0u8; READ_CHUNK];
        while let Ok(read @ 1..) = reader.read(&mut chunk) {
            answer_queries(&chunk[..read], &mut writer);
            bytes.extend_from_slice(&chunk[..read]);
        }
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.contains("hello-pty-marker"),
            "child output arrives before EOF"
        );
    }

    #[test]
    fn resize_keeps_session_usable() {
        let mut request = SpawnRequest::shell();
        request.cols = DEFAULT_COLS;
        request.rows = DEFAULT_ROWS;
        let spawned = spawn_pty(&request);
        assert!(spawned.is_ok(), "pty spawns in this environment");
        let Ok(spawned) = spawned else {
            return;
        };
        assert!(spawned.resize(100, 40).is_ok());
        assert!(spawned.resize(DEFAULT_COLS, DEFAULT_ROWS).is_ok());
        spawned.killer.kill();
    }

    #[test]
    fn reader_loop_collects_and_reaps() {
        let request = echo_request("loop-marker");
        let spawned = spawn_pty(&request);
        assert!(spawned.is_ok(), "pty spawns in this environment");
        let Ok(mut spawned) = spawned else {
            return;
        };
        let watcher = watch_exit(spawned.child, spawned.master);
        assert!(watcher.is_ok(), "the exit watcher starts");
        let Ok(watcher) = watcher else {
            return;
        };
        let cancelled = AtomicBool::new(false);
        let mut collected = Vec::new();
        let writer = &mut spawned.writer;
        run_reader_loop(&mut spawned.reader, &cancelled, |bytes| {
            answer_queries(bytes, writer);
            collected.extend_from_slice(bytes);
            true
        });
        let code = watcher.join().ok().flatten();
        let text = String::from_utf8_lossy(&collected);
        assert!(text.contains("loop-marker"), "loop forwards child bytes");
        assert_eq!(code, Some(0));
    }

    struct CountingWriter {
        count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl std::io::Write for CountingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.count
                .fetch_add(buf.len(), std::sync::atomic::Ordering::SeqCst);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn writer_loop_drains_then_returns() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let writer = CountingWriter {
            count: std::sync::Arc::clone(&count),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let resized = GridSize {
            cols: 100,
            rows: 40,
            ..GridSize::default()
        };
        assert!(tx.send(PtyInput::Bytes(b"abc".to_vec())).is_ok());
        assert!(tx.send(PtyInput::Resize(resized)).is_ok());
        assert!(tx.send(PtyInput::Bytes(b"de".to_vec())).is_ok());
        drop(tx);
        let mut boxed: Box<dyn Write + Send> = Box::new(writer);
        let mut sizes = Vec::new();
        run_writer_loop(&mut boxed, rx, |size| sizes.push(size));
        assert_eq!(
            count.load(std::sync::atomic::Ordering::SeqCst),
            5,
            "every queued batch reaches the sink"
        );
        assert_eq!(
            sizes,
            vec![resized],
            "resize requests reach the master owner"
        );
    }

    #[test]
    fn default_shell_names_a_program() {
        let (program, _) = default_shell();
        assert!(!program.is_empty(), "default shell is configured");
    }

    #[test]
    fn wsl_descriptor_targets_bash() {
        let (program, args) = wsl_shell(None);
        assert_eq!(program, "wsl.exe");
        assert!(args.contains(&"bash".to_owned()));
        let (_, named) = wsl_shell(Some("Ubuntu"));
        assert!(named.contains(&"Ubuntu".to_owned()));
    }
}
