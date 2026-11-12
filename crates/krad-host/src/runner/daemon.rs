use krad_domain::{CloudProcessId, JournalOffset};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use super::clients::{ensure_private_dir, is_paired};
use super::journal::Journal;
use super::listing::list_processes;
use super::piped::start_piped;
use super::process::{forward_input, stop_group, stream_output, Tracked};
use super::pty::start_pty;
use super::restart::adopt_previous;
use super::wire::{CloudLaunch, Envelope, Reply, Request};
use super::PROCESS_ENV;
use crate::mux::TerminalSize;

const SOCKET_FILE: &str = "daemon.sock";
pub(super) const PROCESSES_DIR: &str = "processes";
const CONNECTION_THREAD: &str = "krad-cloud-connection";
const NOT_PAIRED: &str = "this installation is not paired with the cloud runner";
const UNKNOWN_PROCESS: &str = "the cloud runner has no process with that id";
const NOT_RUNNING: &str = "that cloud process is not running";
const NO_TERMINAL: &str = "that cloud process has no terminal";
const RESIZE_FAILED: &str = "the cloud process's terminal did not take the new size";

#[must_use]
pub fn runner_socket(root: &Path) -> PathBuf {
    root.join(SOCKET_FILE)
}

fn marked(mut launch: CloudLaunch, id: &CloudProcessId) -> CloudLaunch {
    launch.env.retain(|(name, _)| name != PROCESS_ENV);
    launch
        .env
        .push((PROCESS_ENV.to_owned(), id.as_str().to_owned()));
    launch
}

pub struct RunnerDaemon {
    root: PathBuf,
    processes: Mutex<HashMap<CloudProcessId, Arc<Tracked>>>,
}

impl RunnerDaemon {
    #[must_use]
    pub fn new(root: &Path) -> Arc<Self> {
        Arc::new(Self {
            root: root.to_owned(),
            processes: Mutex::new(adopt_previous(&root.join(PROCESSES_DIR))),
        })
    }

    /// # Errors
    pub fn bind(&self) -> io::Result<UnixListener> {
        ensure_private_dir(&self.root)?;
        let socket = runner_socket(&self.root);
        if socket.exists() {
            fs::remove_file(&socket)?;
        }
        UnixListener::bind(socket)
    }

    pub fn serve(self: &Arc<Self>, listener: &UnixListener) {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let daemon = Arc::clone(self);
            let spawned = thread::Builder::new()
                .name(CONNECTION_THREAD.to_owned())
                .spawn(move || {
                    if let Err(error) = daemon.handle(stream) {
                        tracing::debug!(%error, "a cloud runner connection ended");
                    }
                });
            if let Err(error) = spawned {
                tracing::warn!(%error, "could not serve a cloud runner connection");
            }
        }
    }

    fn handle(&self, mut stream: UnixStream) -> io::Result<()> {
        let envelope = match Envelope::read(&mut stream) {
            Ok(envelope) => envelope,
            Err(error) => return Reply::Refused(error.to_string()).write(&mut stream),
        };
        if !is_paired(&self.root, &envelope.pairing) {
            tracing::debug!(install = %envelope.pairing.install, "refused an unpaired client");
            return Reply::Refused(NOT_PAIRED.to_owned()).write(&mut stream);
        }
        match envelope.request {
            Request::Open { id, from, launch } => self.attach(&id, from, launch, stream),
            Request::Stop { id } => self.stop(&id, &mut stream),
            Request::List => Reply::Listed(list_processes(
                &self.root.join(PROCESSES_DIR),
                &self.processes(),
            ))
            .write(&mut stream),
            Request::Resize { id, size } => self.resize(&id, size).write(&mut stream),
        }
    }

    fn attach(
        &self,
        id: &CloudProcessId,
        from: JournalOffset,
        launch: Option<CloudLaunch>,
        mut stream: UnixStream,
    ) -> io::Result<()> {
        let tracked = match self.open(id, launch) {
            Ok(tracked) => tracked,
            Err(reason) => return Reply::Refused(reason.to_owned()).write(&mut stream),
        };
        let end = tracked.written();
        if from > end {
            return Reply::PastEnd { end }.write(&mut stream);
        }
        Reply::Accepted { end }.write(&mut stream)?;
        forward_input(&tracked, stream.try_clone()?);
        stream_output(&tracked, from, &mut stream)
    }

    fn stop(&self, id: &CloudProcessId, stream: &mut UnixStream) -> io::Result<()> {
        let tracked = self.processes().get(id).cloned();
        let Some(tracked) = tracked else {
            return Reply::Refused(UNKNOWN_PROCESS.to_owned()).write(stream);
        };
        let Some(group) = tracked.running_group() else {
            return Reply::Refused(NOT_RUNNING.to_owned()).write(stream);
        };
        Reply::Done.write(stream)?;
        stop_group(group);
        Ok(())
    }

    fn resize(&self, id: &CloudProcessId, size: TerminalSize) -> Reply {
        let tracked = self.processes().get(id).cloned();
        let Some(tracked) = tracked else {
            return Reply::Refused(UNKNOWN_PROCESS.to_owned());
        };
        match tracked.resize(size) {
            None => Reply::Refused(NO_TERMINAL.to_owned()),
            Some(Ok(())) => Reply::Done,
            Some(Err(error)) => {
                tracing::debug!(%error, %id, "a cloud terminal did not resize");
                Reply::Refused(RESIZE_FAILED.to_owned())
            }
        }
    }

    fn processes(&self) -> MutexGuard<'_, HashMap<CloudProcessId, Arc<Tracked>>> {
        self.processes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn open(
        &self,
        id: &CloudProcessId,
        launch: Option<CloudLaunch>,
    ) -> Result<Arc<Tracked>, &'static str> {
        let mut processes = self.processes();
        if let Some(tracked) = processes.get(id) {
            return Ok(Arc::clone(tracked));
        }
        let journal = Journal::new(self.root.join(PROCESSES_DIR).join(id.as_str()));
        let tracked = if journal.exists() {
            Arc::new(Tracked::finished(journal).map_err(|_| UNKNOWN_PROCESS)?)
        } else {
            let launch = marked(launch.ok_or(UNKNOWN_PROCESS)?, id);
            match launch.terminal {
                Some(size) => start_pty(journal, &launch, size)?,
                None => start_piped(journal, &launch)?,
            }
        };
        processes.insert(id.clone(), Arc::clone(&tracked));
        Ok(tracked)
    }
}
