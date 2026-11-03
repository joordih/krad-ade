use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use super::super::bridge::copy_until_end;
use super::super::channel::Channel;
use super::super::wire::{Stream, TerminalSize};
use super::process::{spawn_pump, stop_group};

const SHELL: &str = "/bin/sh";
const SHELL_COMMAND_FLAG: &str = "-c";
const OPEN_FAILED: &str = "the host could not open a terminal";
const SPAWN_FAILED: &str = "the host could not start the terminal's shell";

fn pty_size(size: TerminalSize) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

pub(in crate::mux) fn run(channel: &Channel, script: &str, size: TerminalSize) {
    let pair = match native_pty_system().openpty(pty_size(size)) {
        Ok(pair) => pair,
        Err(error) => {
            tracing::debug!(%error, "could not open a terminal on the host");
            channel.refuse(OPEN_FAILED);
            return;
        }
    };
    let mut command = CommandBuilder::new(SHELL);
    command.args([SHELL_COMMAND_FLAG, script]);
    let spawned = pair.slave.spawn_command(command);
    drop(pair.slave);
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            tracing::debug!(%error, "could not start a terminal's shell");
            channel.refuse(SPAWN_FAILED);
            return;
        }
    };
    let streams = pair
        .master
        .try_clone_reader()
        .and_then(|reader| Ok((reader, pair.master.take_writer()?)));
    let Ok((reader, writer)) = streams else {
        end(&mut *child);
        channel.refuse(OPEN_FAILED);
        return;
    };
    if channel.accept(None).is_err() {
        end(&mut *child);
        return;
    }
    let output = {
        let mut sink = channel.writer(Stream::Main);
        spawn_pump(move || {
            copy_until_end(reader, &mut sink).ok();
            sink.finish();
        })
    };
    let input = channel.reader(Stream::Main);
    spawn_pump(move || {
        copy_until_end(input, writer).ok();
    });
    let resizer = follow_resizes(channel, pair.master);
    let killer = watch_for_kill(channel, child.process_id());
    let code = child
        .wait()
        .ok()
        .and_then(|status| i32::try_from(status.exit_code()).ok());
    if let Some(output) = output {
        output.join().ok();
    }
    channel.exit(code).ok();
    channel.close();
    for thread in [resizer, killer].into_iter().flatten() {
        thread.join().ok();
    }
}

fn end(child: &mut (dyn Child + Send + Sync)) {
    if child.kill().is_ok() {
        child.wait().ok();
    }
}

fn follow_resizes(
    channel: &Channel,
    master: Box<dyn MasterPty + Send>,
) -> Option<std::thread::JoinHandle<()>> {
    let channel = channel.clone();
    spawn_pump(move || {
        while let Some(size) = channel.next_resize() {
            if let Err(error) = master.resize(pty_size(size)) {
                tracing::debug!(%error, "the host terminal did not take a new size");
            }
        }
    })
}

fn watch_for_kill(channel: &Channel, pid: Option<u32>) -> Option<std::thread::JoinHandle<()>> {
    let channel = channel.clone();
    let pid = pid?;
    spawn_pump(move || {
        if channel.wait_for_kill() {
            stop_group(pid);
        }
    })
}
