use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{ForwardFailure, HostError, Waited};
use crate::local::hide_console_window;
use crate::mux::MuxForward;
use crate::remote_link::RemoteLink;
use crate::run::output_within;
use crate::ssh::{local_spec, remote_spec, Direction, ForwardRequest, SshArgs};

const TUNNEL_POLL: Duration = Duration::from_millis(50);
const ANY_PORT: u16 = 0;
const LOCAL_ATTEMPTS: usize = 3;
const ALLOCATED_PREFIX: &str = "Allocated port ";
const THREAD_REAP: &str = "krad-ade-ssh-reap";
const THREAD_TUNNEL: &str = "krad-ade-ssh-tunnel";

#[derive(Debug)]
struct Multiplexed {
    args: SshArgs,
    direction: Direction,
    spec: String,
    alive: Arc<AtomicBool>,
}

#[derive(Debug)]
enum Carrier {
    Master(Box<Multiplexed>),
    Tunnel(Option<Child>),
    Session(MuxForward),
}

impl Carrier {
    fn is_alive(&mut self) -> bool {
        match self {
            Self::Master(master) => master.alive.load(Ordering::Acquire),
            Self::Tunnel(child) => child
                .as_mut()
                .is_some_and(|child| child.try_wait().is_ok_and(|status| status.is_none())),
            Self::Session(forward) => forward.is_alive(),
        }
    }
}

#[derive(Debug)]
pub struct Forward {
    pub local_port: u16,
    carrier: Carrier,
}

impl Forward {
    #[must_use]
    pub fn is_alive(&mut self) -> bool {
        self.carrier.is_alive()
    }
}

#[derive(Debug)]
pub struct RemoteForward {
    pub remote_port: u16,
    carrier: Carrier,
}

impl RemoteForward {
    #[must_use]
    pub fn is_alive(&mut self) -> bool {
        self.carrier.is_alive()
    }
}

impl Drop for Carrier {
    fn drop(&mut self) {
        match self {
            Self::Master(master) => {
                if let Some(cancel) = master.args.mux_forward_args(
                    master.direction,
                    &master.spec,
                    ForwardRequest::Cancel,
                ) {
                    if let Ok(child) = quiet(&cancel).stderr(Stdio::null()).spawn() {
                        reap(child);
                    }
                }
            }
            Self::Tunnel(child) => {
                if let Some(mut child) = child.take() {
                    child.kill().ok();
                    reap(child);
                }
            }
            Self::Session(_) => {}
        }
    }
}

fn reap(mut child: Child) {
    let reaping = thread::Builder::new()
        .name(THREAD_REAP.to_owned())
        .spawn(move || child.wait().ok());
    drop(reaping);
}

fn quiet(args: &[OsString]) -> Command {
    let mut command = Command::new(SshArgs::program());
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    hide_console_window(&mut command);
    command
}

fn free_local_port() -> Result<u16, HostError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, ANY_PORT))?;
    Ok(listener.local_addr()?.port())
}

impl RemoteLink {
    fn forward_remote_on_master(
        &self,
        remote_port: u16,
        local_port: u16,
        wait: Duration,
    ) -> Option<Result<RemoteForward, HostError>> {
        let request = remote_spec(remote_port, local_port);
        let open = self
            .args
            .mux_forward_args(Direction::Remote, &request, ForwardRequest::Open)?;
        let mut command = quiet(&open);
        command.stdout(Stdio::piped());
        let opened = output_within(command, wait, Waited::Forward).and_then(|output| {
            if !output.status.success() {
                return Err(HostError::Forward(ForwardFailure::Rejected));
            }
            master_granted_port(remote_port, &String::from_utf8_lossy(&output.stdout))
                .ok_or(HostError::Forward(ForwardFailure::NoPortAllocated))
        });
        Some(opened.map(|granted| RemoteForward {
            remote_port: granted,
            carrier: Carrier::Master(Box::new(Multiplexed {
                args: self.args.clone(),
                direction: Direction::Remote,
                spec: request,
                alive: self.liveness(),
            })),
        }))
    }

    /// # Errors
    #[tracing::instrument(skip_all, fields(destination = %self.args.destination, remote_port))]
    pub fn forward_local(&self, remote_port: u16) -> Result<Forward, HostError> {
        let mut last = HostError::Forward(ForwardFailure::Rejected);
        for _ in 0..LOCAL_ATTEMPTS {
            match self.try_forward_local(remote_port) {
                Ok(forward) => return Ok(forward),
                Err(error) => {
                    tracing::debug!(%error, "local forward attempt failed");
                    last = error;
                }
            }
        }
        Err(last)
    }

    fn try_forward_local(&self, remote_port: u16) -> Result<Forward, HostError> {
        if let Some(mux) = self.multiplexed() {
            let (forward, local_port) =
                MuxForward::local(&mux, remote_port, self.args.timeouts.forward.duration())?;
            return Ok(Forward {
                local_port,
                carrier: Carrier::Session(forward),
            });
        }
        self.refuse_ssh()?;
        let local_port = free_local_port()?;
        let spec = local_spec(local_port, remote_port);
        let wait = self.args.timeouts.forward.duration();
        if let Some(open) =
            self.args
                .mux_forward_args(Direction::Local, &spec, ForwardRequest::Open)
        {
            let output = output_within(quiet(&open), wait, Waited::Forward)?;
            if !output.status.success() {
                return Err(HostError::Forward(ForwardFailure::Rejected));
            }
            return Ok(Forward {
                local_port,
                carrier: Carrier::Master(Box::new(Multiplexed {
                    args: self.args.clone(),
                    direction: Direction::Local,
                    spec,
                    alive: self.liveness(),
                })),
            });
        }
        let child = quiet(&self.args.tunnel_args(Direction::Local, &spec))
            .stderr(Stdio::null())
            .spawn()?;
        let mut forward = Forward {
            local_port,
            carrier: Carrier::Tunnel(Some(child)),
        };
        wait_until_listening(&mut forward, wait)?;
        Ok(forward)
    }

    /// # Errors
    #[tracing::instrument(skip_all, fields(destination = %self.args.destination, local_port))]
    pub fn forward_remote(
        &self,
        local_port: u16,
        preferred: Option<u16>,
    ) -> Result<RemoteForward, HostError> {
        let wait = self.args.timeouts.forward.duration();
        if let Some(mux) = self.multiplexed() {
            let (forward, remote_port) = MuxForward::remote(&mux, local_port, wait)?;
            return Ok(RemoteForward {
                remote_port,
                carrier: Carrier::Session(forward),
            });
        }
        self.refuse_ssh()?;
        if let Some(port) = preferred.filter(|port| *port != ANY_PORT) {
            match self.forward_remote_on_master(port, local_port, wait) {
                Some(Ok(forward)) => return Ok(forward),
                Some(Err(error)) => {
                    tracing::debug!(%error, port, "the host's previous bus port is taken");
                }
                None => {}
            }
        }
        if let Some(forward) = self.forward_remote_on_master(ANY_PORT, local_port, wait) {
            return forward;
        }
        let request = remote_spec(ANY_PORT, local_port);
        let mut child = quiet(&self.args.tunnel_args(Direction::Remote, &request)).spawn()?;
        let Some(stderr) = child.stderr.take() else {
            return Err(HostError::Forward(ForwardFailure::NoPortAllocated));
        };
        let carrier = Carrier::Tunnel(Some(child));
        let (found, port) = mpsc::channel();
        thread::Builder::new()
            .name(THREAD_TUNNEL.to_owned())
            .spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if let Some(remote_port) = tunnel_allocated_port(&line) {
                        found.send(remote_port).ok();
                    }
                }
            })?;
        let remote_port = port
            .recv_timeout(wait)
            .map_err(|_| HostError::Forward(ForwardFailure::NoPortAllocated))?;
        Ok(RemoteForward {
            remote_port,
            carrier,
        })
    }
}

fn master_allocated_port(stdout: &str) -> Option<u16> {
    stdout.trim().parse().ok().filter(|port| *port != ANY_PORT)
}

fn master_granted_port(requested: u16, stdout: &str) -> Option<u16> {
    if requested == ANY_PORT {
        return master_allocated_port(stdout);
    }
    Some(requested)
}

fn tunnel_allocated_port(line: &str) -> Option<u16> {
    let at = line.find(ALLOCATED_PREFIX)?;
    line[at + ALLOCATED_PREFIX.len()..]
        .split_whitespace()
        .next()?
        .parse()
        .ok()
        .filter(|port| *port != ANY_PORT)
}

fn wait_until_listening(forward: &mut Forward, wait: Duration) -> Result<(), HostError> {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        if !forward.is_alive() {
            return Err(HostError::Forward(ForwardFailure::Rejected));
        }
        if TcpStream::connect((Ipv4Addr::LOCALHOST, forward.local_port)).is_ok() {
            return Ok(());
        }
        thread::sleep(TUNNEL_POLL);
    }
    Err(HostError::Forward(ForwardFailure::NeverListened))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_master_prints_the_allocated_port_alone() {
        assert_eq!(master_allocated_port("53705\n"), Some(53_705));
        assert_eq!(master_allocated_port(""), None);
        assert_eq!(master_allocated_port("0"), None);
    }

    #[test]
    fn a_requested_port_is_granted_as_asked_and_a_dynamic_one_as_printed() {
        assert_eq!(master_granted_port(41_000, ""), Some(41_000));
        assert_eq!(master_granted_port(ANY_PORT, "53705\n"), Some(53_705));
        assert_eq!(master_granted_port(ANY_PORT, ""), None);
    }

    #[test]
    fn a_tunnel_reports_the_allocated_port_on_stderr() {
        assert_eq!(
            tunnel_allocated_port("Allocated port 53707 for remote forward to 127.0.0.1:9"),
            Some(53_707)
        );
        assert_eq!(
            tunnel_allocated_port("debug1: Allocated port 41000 for remote forward to x"),
            Some(41_000)
        );
        assert_eq!(tunnel_allocated_port("Warning: Permanently added"), None);
    }
}
