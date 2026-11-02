use krad_host::{HostCommand, HostError, HostLink, TerminalControl};
use krad_session::{
    GridSize, PtyError, PtyKill, PtyOpener, PtyResize, PtyWait, SharedKiller, SharedMaster,
    SpawnedPty,
};

struct RemoteResize(TerminalControl);
struct RemoteWait(TerminalControl);
struct RemoteKill(TerminalControl);

impl PtyResize for RemoteResize {
    fn resize(&self, size: GridSize) -> Result<(), PtyError> {
        self.0.resize(size.cols, size.rows);
        Ok(())
    }
}

impl PtyWait for RemoteWait {
    fn wait(&mut self) -> Option<i32> {
        self.0.wait()
    }
}

impl PtyKill for RemoteKill {
    fn kill(&mut self) {
        self.0.kill();
    }
}

fn spawn_failure(error: HostError) -> PtyError {
    PtyError::Spawn(Box::new(error))
}

pub(in crate::environment_shell) fn multiplexed_opener(
    link: &HostLink,
    command: HostCommand,
) -> Option<PtyOpener> {
    let HostLink::Ssh(ssh) = link else {
        return None;
    };
    if !ssh.is_multiplexed() {
        return None;
    }
    let link = link.clone();
    Some(Box::new(move |cols, rows| {
        let terminal = command
            .open_terminal(&link, cols, rows)
            .map_err(spawn_failure)?
            .ok_or_else(|| spawn_failure(HostError::NotConnected))?;
        let control = terminal.control;
        Ok(SpawnedPty {
            reader: terminal.reader,
            writer: terminal.writer,
            master: SharedMaster::new(Box::new(RemoteResize(control.clone()))),
            killer: SharedKiller::new(Box::new(RemoteKill(control.clone()))),
            child: Box::new(RemoteWait(control)),
            pid: None,
        })
    }))
}
