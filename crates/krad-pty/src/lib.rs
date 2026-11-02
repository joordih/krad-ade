use krad_domain::INHERITED_SESSION_VARS;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use thiserror::Error;

mod handles;
#[cfg(not(windows))]
mod unix;
#[cfg(windows)]
mod windows;
mod wsl;

pub use handles::{resize_master, PtyKill, PtyResize, PtyWait, SharedKiller, SharedMaster};

#[cfg(not(windows))]
pub use unix::{clear_screen_prefix, default_shell, login_shell_env, SHELL_QUOTING};
#[cfg(not(windows))]
use unix::{executable_names, is_executable};
#[cfg(windows)]
pub use windows::{clear_screen_prefix, default_shell, login_shell_env, SHELL_QUOTING};
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
pub const TERMINAL_ENV: [(&str, &str); 2] =
    [(TERM_ENV, TERM_VALUE), (COLORTERM_ENV, COLORTERM_VALUE)];

pub type PlatformError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Error)]
pub enum PtyError {
    #[error("cannot spawn pty child: {0}")]
    Spawn(#[source] PlatformError),
    #[error("pty I/O failure: {0}")]
    Io(#[from] std::io::Error),
}

impl PtyError {
    pub(crate) fn platform(error: impl Into<PlatformError>) -> Self {
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

pub type PtyOpener = Box<dyn FnOnce(u16, u16) -> Result<SpawnedPty, PtyError> + Send>;

pub struct SpawnedPty {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub master: SharedMaster,
    pub killer: SharedKiller,
    pub child: Box<dyn PtyWait>,
    pub pid: Option<u32>,
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
    let killer = SharedKiller::new(Box::new(child.clone_killer()));
    let reader = pair.master.try_clone_reader().map_err(PtyError::platform)?;
    let writer = pair.master.take_writer().map_err(PtyError::platform)?;
    Ok(SpawnedPty {
        reader,
        writer,
        master: SharedMaster::new(Box::new(pair.master)),
        killer,
        child: Box::new(child),
        pid,
    })
}

/// # Errors
pub fn watch_exit(
    mut child: Box<dyn PtyWait>,
    master: SharedMaster,
) -> Result<JoinHandle<Option<i32>>, PtyError> {
    let handle = std::thread::Builder::new()
        .name(EXIT_THREAD_NAME.to_owned())
        .spawn(move || {
            let code = child.wait();
            master.close();
            code
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
    pub(crate) const fn pixels(self) -> (u16, u16) {
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

    fn request(program: String, args: Vec<String>) -> SpawnRequest {
        SpawnRequest {
            program,
            args,
            cwd: None,
            env: Vec::new(),
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
        }
    }

    #[cfg(unix)]
    fn echo_request(marker: &str) -> SpawnRequest {
        request(
            "/bin/sh".to_owned(),
            vec!["-c".to_owned(), format!("echo {marker}")],
        )
    }

    #[cfg(windows)]
    fn echo_request(marker: &str) -> SpawnRequest {
        request(
            "cmd.exe".to_owned(),
            vec!["/C".to_owned(), format!("echo {marker}")],
        )
    }

    fn grid(cols: u16, rows: u16) -> GridSize {
        GridSize {
            cols,
            rows,
            ..GridSize::default()
        }
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
    fn resize_keeps_session_usable() {
        let (program, args) = default_shell();
        let spawned = spawn_pty(&request(program, args));
        assert!(spawned.is_ok(), "pty spawns in this environment");
        let Ok(spawned) = spawned else {
            return;
        };
        assert!(spawned.master.resize(grid(100, 40)).is_ok());
        assert!(spawned
            .master
            .resize(grid(DEFAULT_COLS, DEFAULT_ROWS))
            .is_ok());
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
    fn wsl_descriptor_targets_bash() {
        let (program, args) = wsl_shell(None);
        assert_eq!(program, "wsl.exe");
        assert!(args.contains(&"bash".to_owned()));
        let (_, named) = wsl_shell(Some("Ubuntu"));
        assert!(named.contains(&"Ubuntu".to_owned()));
    }
}
