use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use std::sync::Arc;

use super::journal::Journal;
use super::process::{spawn_pump, Tracked, SPAWN_FAILED};
use super::wire::CloudLaunch;
use crate::mux::TerminalSize;

pub(super) fn pty_size(size: TerminalSize) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

pub(super) fn start_pty(
    journal: Journal,
    launch: &CloudLaunch,
    size: TerminalSize,
) -> Result<Arc<Tracked>, &'static str> {
    let (output, _) = journal.create().map_err(|_| SPAWN_FAILED)?;
    journal.mark_terminal().map_err(|_| SPAWN_FAILED)?;
    let pair = native_pty_system()
        .openpty(pty_size(size))
        .map_err(|error| {
            tracing::debug!(%error, "could not open a terminal for a cloud process");
            SPAWN_FAILED
        })?;
    let mut command = CommandBuilder::new(&launch.program);
    command.args(&launch.args);
    for (name, value) in &launch.env {
        command.env(name, value);
    }
    command.cwd(&launch.cwd);
    let spawned = pair.slave.spawn_command(command);
    drop(pair.slave);
    let mut child = spawned.map_err(|error| {
        tracing::debug!(%error, "could not start a cloud terminal process");
        SPAWN_FAILED
    })?;
    let streams = pair
        .master
        .try_clone_reader()
        .and_then(|reader| Ok((reader, pair.master.take_writer()?)));
    let Ok((reader, writer)) = streams else {
        end(&mut *child);
        return Err(SPAWN_FAILED);
    };
    let tracked = Arc::new(Tracked::live(
        journal,
        writer,
        Some(pair.master),
        child.process_id(),
    ));
    spawn_pump(&tracked, reader, output, move || {
        child
            .wait()
            .ok()
            .and_then(|status| i32::try_from(status.exit_code()).ok())
    })?;
    Ok(tracked)
}

fn end(child: &mut (dyn Child + Send + Sync)) {
    if child.kill().is_ok() {
        child.wait().ok();
    }
}
