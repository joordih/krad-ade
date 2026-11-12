use std::net::{Ipv4Addr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::thread;
use std::time::Duration;

use super::bridge::splice_tcp;
use super::carrier::MuxCarrier;
use super::channel::Channel;
use super::wire::OpenRequest;
use crate::error::{ForwardFailure, HostError};

const ACCEPT_POLL: Duration = Duration::from_millis(100);
const LISTEN_THREAD: &str = "krad-ade-mux-forward";

#[derive(Debug)]
enum Kind {
    Local { stop: Arc<AtomicBool> },
    Remote { channel: ChannelGuard },
}

pub(crate) struct ChannelGuard {
    channel: Channel,
    carrier: Weak<MuxCarrier>,
}

impl std::fmt::Debug for ChannelGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelGuard")
            .field("channel", &self.channel.id())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(crate) struct MuxForward {
    kind: Kind,
    carrier: Weak<MuxCarrier>,
}

impl MuxForward {
    pub(crate) fn local(
        carrier: &Arc<MuxCarrier>,
        local_port: u16,
        remote_port: u16,
        wait: Duration,
    ) -> Result<(Self, u16), HostError> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, local_port))?;
        listener.set_nonblocking(true)?;
        let local_port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let weak = Arc::downgrade(carrier);
        thread::Builder::new()
            .name(LISTEN_THREAD.to_owned())
            .spawn(move || accept_loop(&listener, &weak, remote_port, wait, &stopping))?;
        Ok((
            Self {
                kind: Kind::Local { stop },
                carrier: Arc::downgrade(carrier),
            },
            local_port,
        ))
    }

    pub(crate) fn remote(
        carrier: &Arc<MuxCarrier>,
        local_port: u16,
        wait: Duration,
    ) -> Result<(Self, u16), HostError> {
        let (channel, port) = carrier.open(OpenRequest::Listen, wait)?;
        let remote_port = port.ok_or(HostError::Forward(ForwardFailure::NoPortAllocated))?;
        carrier.listen_for(channel.id(), local_port);
        Ok((
            Self {
                kind: Kind::Remote {
                    channel: ChannelGuard {
                        channel,
                        carrier: Arc::downgrade(carrier),
                    },
                },
                carrier: Arc::downgrade(carrier),
            },
            remote_port,
        ))
    }

    pub(crate) fn is_alive(&self) -> bool {
        let carrier_alive = self
            .carrier
            .upgrade()
            .is_some_and(|carrier| carrier.is_alive());
        carrier_alive
            && match &self.kind {
                Kind::Local { stop } => !stop.load(Ordering::Acquire),
                Kind::Remote { channel } => !channel.channel.is_closed(),
            }
    }
}

impl Drop for MuxForward {
    fn drop(&mut self) {
        if let Kind::Local { stop } = &self.kind {
            stop.store(true, Ordering::Release);
        }
    }
}

impl Drop for ChannelGuard {
    fn drop(&mut self) {
        if let Some(carrier) = self.carrier.upgrade() {
            carrier.forget_listener(self.channel.id());
        }
        self.channel.close();
    }
}

fn accept_loop(
    listener: &TcpListener,
    carrier: &Weak<MuxCarrier>,
    remote_port: u16,
    wait: Duration,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::Acquire) {
        let Some(alive) = carrier.upgrade().filter(|carrier| carrier.is_alive()) else {
            stop.store(true, Ordering::Release);
            return;
        };
        drop(alive);
        match listener.accept() {
            Ok((tcp, _)) => {
                let carrier = carrier.clone();
                let spawned = thread::Builder::new().name(LISTEN_THREAD.to_owned()).spawn(
                    move || {
                        let Some(carrier) = carrier.upgrade() else {
                            return;
                        };
                        if tcp.set_nonblocking(false).is_err() {
                            return;
                        }
                        match carrier.open(OpenRequest::Connect { port: remote_port }, wait) {
                            Ok((channel, _)) => {
                                drop(carrier);
                                splice_tcp(&channel, &tcp).ok();
                            }
                            Err(error) => {
                                tracing::debug!(%error, "the host refused a forwarded connection");
                            }
                        }
                    },
                );
                if let Err(error) = spawned {
                    tracing::warn!(%error, "could not forward a connection");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL);
            }
            Err(error) => {
                tracing::debug!(%error, "the local forward listener failed");
                stop.store(true, Ordering::Release);
                return;
            }
        }
    }
}
