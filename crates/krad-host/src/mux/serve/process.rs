use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::super::bridge::copy_until_end;
use super::super::channel::Channel;
use super::super::wire::Stream;

const SHELL: &str = "/bin/sh";
const SHELL_COMMAND_FLAG: &str = "-c";
const KILL_PROGRAM: &str = "kill";
const TERMINATE_SIGNAL: &str = "-TERM";
const KILL_SIGNAL: &str = "-KILL";
const END_OF_OPTIONS: &str = "--";
const OWN_PROCESS_GROUP: i32 = 0;
const KILL_GRACE: Duration = Duration::from_secs(2);
const PUMP_THREAD: &str = "krad-ade-mux-process";
const SPAWN_FAILED: &str = "the host could not start the command";
const SIGNAL_EXIT_BASE: i32 = 128;

pub(crate) enum Launch {
    Script(String),
    Program(PathBuf, Vec<String>),
}

impl Launch {
    fn command(&self) -> Command {
        match self {
            Self::Script(script) => {
                let mut command = Command::new(SHELL);
                command.args([SHELL_COMMAND_FLAG, script]);
                command
            }
            Self::Program(program, args) => {
                let mut command = Command::new(program);
                command.args(args);
                command
            }
        }
    }
}

pub(super) fn run(channel: &Channel, launch: &Launch) {
    let mut command = launch.command();
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(OWN_PROCESS_GROUP);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            tracing::debug!(%error, "could not start a requested command");
            channel.refuse(SPAWN_FAILED);
            return;
        }
    };
    if channel.accept(None).is_err() {
        stop_group(child.id());
        child.wait().ok();
        return;
    }
    let finished = Arc::new(AtomicBool::new(false));
    feed_stdin(channel, &mut child);
    let pumps = start_output_pumps(channel, &mut child);
    let watcher = watch_for_kill(channel, child.id(), Arc::clone(&finished));
    let status = child.wait();
    finished.store(true, Ordering::Release);
    for pump in pumps.into_iter().flatten() {
        pump.join().ok();
    }
    let code = status.ok().map(|status| {
        status
            .code()
            .unwrap_or_else(|| SIGNAL_EXIT_BASE + status.signal().unwrap_or_default())
    });
    channel.exit(code).ok();
    channel.close();
    if let Some(watcher) = watcher {
        watcher.join().ok();
    }
}

fn feed_stdin(channel: &Channel, child: &mut Child) {
    if let Some(stdin) = child.stdin.take() {
        let reader = channel.reader(Stream::Main);
        spawn_pump(move || {
            copy_until_end(reader, stdin).ok();
        });
    }
}

fn start_output_pumps(channel: &Channel, child: &mut Child) -> Vec<Option<thread::JoinHandle<()>>> {
    let mut pumps = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let mut writer = channel.writer(Stream::Main);
        pumps.push(spawn_pump(move || {
            copy_until_end(stdout, &mut writer).ok();
            writer.finish();
        }));
    }
    if let Some(stderr) = child.stderr.take() {
        let mut writer = channel.writer(Stream::Errors);
        pumps.push(spawn_pump(move || {
            copy_until_end(stderr, &mut writer).ok();
            writer.finish();
        }));
    }
    pumps
}

fn spawn_pump(work: impl FnOnce() + Send + 'static) -> Option<thread::JoinHandle<()>> {
    thread::Builder::new()
        .name(PUMP_THREAD.to_owned())
        .spawn(work)
        .ok()
}

fn watch_for_kill(
    channel: &Channel,
    pid: u32,
    finished: Arc<AtomicBool>,
) -> Option<thread::JoinHandle<()>> {
    let channel = channel.clone();
    spawn_pump(move || {
        channel.wait_for_kill();
        if !finished.load(Ordering::Acquire) {
            stop_group(pid);
        }
    })
}

fn stop_group(pid: u32) {
    let group = format!("-{pid}");
    signal_group(TERMINATE_SIGNAL, &group);
    thread::sleep(KILL_GRACE);
    signal_group(KILL_SIGNAL, &group);
}

fn signal_group(signal: &str, group: &str) {
    Command::new(KILL_PROGRAM)
        .args([signal, END_OF_OPTIONS, group])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok();
}
