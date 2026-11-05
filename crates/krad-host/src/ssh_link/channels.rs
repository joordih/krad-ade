use std::process::{Child, Stdio};
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex, TryLockError};
use std::thread;

use super::{lock, ssh_command, SshLink};
use crate::error::{HostError, Waited};
use crate::mux::OpenRequest;
use crate::mux::{Channel, Stream};
use crate::sftp::{Sftp, SftpError};
use crate::ssh::Interaction;

pub(super) const HOME_DIR: &str = ".";
const SFTP_ERRORS_THREAD: &str = "krad-ade-sftp-errors";
const HANDSHAKE_THREAD: &str = "krad-ade-sftp-handshake";

enum Keep {
    Process(Child),
    Multiplexed(Channel),
}

impl Keep {
    fn has_ended(&mut self) -> bool {
        match self {
            Self::Process(child) => !matches!(child.try_wait(), Ok(None)),
            Self::Multiplexed(channel) => channel.is_closed(),
        }
    }
}

pub(super) struct Carrier {
    keep: Keep,
    pub(super) sftp: Arc<Sftp>,
}

impl Drop for Carrier {
    fn drop(&mut self) {
        match &mut self.keep {
            Keep::Process(child) => {
                if child.kill().is_ok() {
                    child.wait().ok();
                }
            }
            Keep::Multiplexed(channel) => channel.close(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lane {
    Interactive,
    Bulk,
}

impl SshLink {
    #[tracing::instrument(skip_all, fields(destination = %self.args.destination))]
    pub(super) fn open_carrier(&self) -> Result<Carrier, HostError> {
        if let Some(mux) = self.multiplexed() {
            let (channel, _) =
                mux.open(OpenRequest::Sftp, self.args.timeouts.connect.duration())?;
            drain_errors(&channel)?;
            let sftp =
                self.handshake_within(channel.reader(Stream::Main), channel.writer(Stream::Main));
            return match sftp {
                Ok(sftp) => Ok(Carrier {
                    keep: Keep::Multiplexed(channel),
                    sftp: Arc::new(sftp),
                }),
                Err(error) => {
                    channel.close();
                    Err(error)
                }
            };
        }
        self.refuse_ssh()?;
        let mut child = ssh_command(&self.args.base(Interaction::Subsystem))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let pipes = child.stdin.take().zip(child.stdout.take());
        let Some((stdin, stdout)) = pipes else {
            child.kill().ok();
            child.wait().ok();
            return Err(HostError::NotConnected);
        };
        match self.handshake_within(stdout, stdin) {
            Ok(sftp) => {
                tracing::debug!("sftp channel open");
                Ok(Carrier {
                    keep: Keep::Process(child),
                    sftp: Arc::new(sftp),
                })
            }
            Err(error) => {
                child.kill().ok();
                child.wait().ok();
                Err(error)
            }
        }
    }

    fn handshake_within(
        &self,
        input: impl std::io::Read + Send + 'static,
        output: impl std::io::Write + Send + 'static,
    ) -> Result<Sftp, HostError> {
        let request = self.args.timeouts.request.duration();
        let (sender, shaken) = mpsc::channel();
        thread::Builder::new()
            .name(HANDSHAKE_THREAD.to_owned())
            .spawn(move || {
                sender.send(Sftp::handshake(input, output, request)).ok();
            })?;
        match shaken.recv_timeout(self.args.timeouts.connect.duration()) {
            Ok(result) => Ok(result?),
            Err(_) => Err(HostError::TimedOut(Waited::Connect)),
        }
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        if !self.open.load(Ordering::Acquire) {
            return false;
        }
        let mut carrier = match self.interactive.try_lock() {
            Ok(carrier) => carrier,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return self.open.load(Ordering::Acquire),
        };
        let mux_gone = self.multiplexed().is_some_and(|mux| !mux.is_alive());
        let ended = mux_gone
            || carrier
                .as_mut()
                .is_none_or(|carrier| carrier.sftp.is_closed() || carrier.keep.has_ended());
        if ended {
            carrier.take();
            self.open.store(false, Ordering::Release);
        }
        !ended
    }

    fn lane(&self, lane: Lane) -> &Mutex<Option<Carrier>> {
        match lane {
            Lane::Interactive => &self.interactive,
            Lane::Bulk => &self.bulk,
        }
    }

    fn channel(&self, lane: Lane) -> Result<Arc<Sftp>, HostError> {
        if !self.open.load(Ordering::Acquire) {
            return Err(HostError::NotConnected);
        }
        let mut slot = lock(self.lane(lane));
        if let Some(carrier) = slot.as_ref().filter(|carrier| !carrier.sftp.is_closed()) {
            return Ok(Arc::clone(&carrier.sftp));
        }
        if lane == Lane::Interactive {
            slot.take();
            self.open.store(false, Ordering::Release);
            return Err(HostError::NotConnected);
        }
        let carrier = self.open_carrier()?;
        let sftp = Arc::clone(&carrier.sftp);
        *slot = Some(carrier);
        Ok(sftp)
    }

    fn drop_lane(&self, lane: Lane) {
        lock(self.lane(lane)).take();
        if lane == Lane::Interactive {
            self.open.store(false, Ordering::Release);
            tracing::warn!(destination = %self.args.destination, "the sftp channel broke");
        }
    }

    pub(crate) fn with_sftp<T>(
        &self,
        run: impl FnOnce(&Sftp) -> Result<T, SftpError>,
    ) -> Result<T, HostError> {
        self.on_lane(Lane::Interactive, run)
    }

    pub(crate) fn with_bulk<T>(
        &self,
        run: impl FnOnce(&Sftp) -> Result<T, SftpError>,
    ) -> Result<T, HostError> {
        self.on_lane(Lane::Bulk, run)
    }

    fn on_lane<T>(
        &self,
        lane: Lane,
        run: impl FnOnce(&Sftp) -> Result<T, SftpError>,
    ) -> Result<T, HostError> {
        let sftp = self.channel(lane)?;
        let result = run(&sftp).map_err(HostError::from);
        if let Err(error) = &result {
            if error.breaks_the_channel() || sftp.is_closed() {
                self.drop_lane(lane);
            }
        }
        result
    }
}

fn drain_errors(channel: &crate::mux::Channel) -> Result<(), HostError> {
    let mut errors = channel.reader(Stream::Errors);
    std::thread::Builder::new()
        .name(SFTP_ERRORS_THREAD.to_owned())
        .spawn(move || {
            let mut lines = std::io::BufReader::new(&mut errors);
            let mut line = String::new();
            while std::io::BufRead::read_line(&mut lines, &mut line).is_ok_and(|read| read > 0) {
                tracing::trace!(message = line.trim_end(), "host sftp-server said");
                line.clear();
            }
        })?;
    Ok(())
}
