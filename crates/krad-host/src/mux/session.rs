use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use super::channel::{Channel, ChannelShared};
use super::wire::{Body, Frame, OpenRequest, CONTROL_CHANNEL, PROTOCOL_VERSION};
use super::MuxError;
use crate::ssh_link::lock;

const READER_THREAD: &str = "krad-ade-mux-reader";
const CHANNEL_ID_STEP: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Client,
    Host,
}

impl Side {
    const fn first_channel(self) -> u32 {
        match self {
            Self::Client => 2,
            Self::Host => 1,
        }
    }

    const fn owns(self, id: u32) -> bool {
        id % CHANNEL_ID_STEP == self.first_channel() % CHANNEL_ID_STEP
    }

    const fn peer(self) -> Self {
        match self {
            Self::Client => Self::Host,
            Self::Host => Self::Client,
        }
    }
}

pub(crate) struct Incoming {
    pub(crate) channel: Channel,
    pub(crate) request: OpenRequest,
}

pub(crate) struct SessionShared {
    writer: Mutex<Box<dyn Write + Send>>,
    channels: Mutex<HashMap<u32, Arc<ChannelShared>>>,
    abandoned: Mutex<HashSet<u32>>,
    side: Side,
    next_id: AtomicU32,
    closed: AtomicBool,
    incoming: Mutex<Option<mpsc::Sender<Incoming>>>,
    closer: Mutex<Option<Closer>>,
}

pub(crate) type Closer = Box<dyn FnOnce() + Send>;

impl SessionShared {
    pub(crate) fn send(&self, frame: &Frame) -> Result<(), MuxError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(MuxError::Closed);
        }
        let bytes = frame.encode()?;
        let mut writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        let written = writer.write_all(&bytes).and_then(|()| writer.flush());
        drop(writer);
        if let Err(error) = written {
            self.shut_down();
            return Err(MuxError::Io(error));
        }
        Ok(())
    }

    pub(crate) fn forget(&self, id: u32) {
        lock(&self.channels).remove(&id);
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn shut_down(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        *self.writer.lock().unwrap_or_else(PoisonError::into_inner) = Box::new(io::sink());
        if let Some(close) = lock(&self.closer).take() {
            close();
        }
        let channels: Vec<_> = lock(&self.channels)
            .drain()
            .map(|(_, shared)| shared)
            .collect();
        for shared in channels {
            shared.peer_closed();
        }
        lock(&self.incoming).take();
        tracing::debug!("multiplexer session ended");
    }

    fn register(&self, id: u32) -> Option<Arc<ChannelShared>> {
        let shared = Arc::new(ChannelShared::new(id));
        let mut channels = lock(&self.channels);
        if channels.contains_key(&id) || lock(&self.abandoned).contains(&id) {
            return None;
        }
        channels.insert(id, Arc::clone(&shared));
        Some(shared)
    }

    fn abandon(&self, id: u32) {
        self.forget(id);
        lock(&self.abandoned).insert(id);
        self.send(&Frame::new(id, Body::Kill)).ok();
        self.send(&Frame::new(id, Body::Close)).ok();
    }

    fn settle_abandoned(&self, id: u32, body: &Body) -> bool {
        let mut abandoned = lock(&self.abandoned);
        if !abandoned.contains(&id) {
            return false;
        }
        match body {
            Body::Opened { .. } => {
                drop(abandoned);
                self.send(&Frame::new(id, Body::Close)).ok();
            }
            Body::Close | Body::Refused { .. } => {
                abandoned.remove(&id);
            }
            _ => {}
        }
        true
    }

    fn find(&self, id: u32) -> Option<Arc<ChannelShared>> {
        lock(&self.channels).get(&id).cloned()
    }
}

#[derive(Clone)]
pub(crate) struct Session {
    shared: Arc<SessionShared>,
}

impl Session {
    pub(crate) fn start(
        input: impl Read + Send + 'static,
        mut output: impl Write + Send + 'static,
        side: Side,
        version: &str,
        handshake: Duration,
        closer: Option<Closer>,
    ) -> Result<(Self, mpsc::Receiver<Incoming>, String), MuxError> {
        let hello = Frame::new(
            CONTROL_CHANNEL,
            Body::Hello {
                protocol: PROTOCOL_VERSION,
                version: version.to_owned(),
            },
        );
        if side == Side::Host {
            hello.write_to(&mut output)?;
        }
        let (input, peer) = match read_hello(input, handshake) {
            Ok(read) => read,
            Err(error) => {
                if let Some(close) = closer {
                    close();
                }
                return Err(error);
            }
        };
        if side == Side::Client {
            hello.write_to(&mut output)?;
        }
        let (sender, incoming) = mpsc::channel();
        let shared = Arc::new(SessionShared {
            writer: Mutex::new(Box::new(output)),
            channels: Mutex::new(HashMap::new()),
            abandoned: Mutex::new(HashSet::new()),
            side,
            next_id: AtomicU32::new(side.first_channel()),
            closed: AtomicBool::new(false),
            incoming: Mutex::new(Some(sender)),
            closer: Mutex::new(closer),
        });
        let reading = Arc::clone(&shared);
        let mut input = input;
        thread::Builder::new()
            .name(READER_THREAD.to_owned())
            .spawn(move || {
                dispatch(&reading, &mut input);
                reading.shut_down();
            })?;
        Ok((Self { shared }, incoming, peer))
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.shared.is_closed()
    }

    pub(crate) fn close(&self) {
        self.shared.shut_down();
    }

    pub(crate) fn open(
        &self,
        request: OpenRequest,
        wait: Duration,
    ) -> Result<(Channel, Option<u16>), MuxError> {
        let id = self
            .shared
            .next_id
            .fetch_add(CHANNEL_ID_STEP, Ordering::Relaxed);
        let shared = self.shared.register(id).ok_or(MuxError::Protocol)?;
        let channel = Channel::new(Arc::clone(&self.shared), shared);
        if let Err(error) = self.shared.send(&Frame::new(id, Body::Open(request))) {
            self.shared.forget(id);
            return Err(error);
        }
        match channel.wait_opened(wait) {
            Ok(port) => Ok((channel, port)),
            Err(MuxError::TimedOut) => {
                self.shared.abandon(id);
                Err(MuxError::TimedOut)
            }
            Err(error) => {
                self.shared.forget(id);
                Err(error)
            }
        }
    }
}

fn read_hello<R: Read + Send + 'static>(
    mut input: R,
    wait: Duration,
) -> Result<(R, String), MuxError> {
    let (sender, received) = mpsc::channel();
    thread::Builder::new()
        .name(READER_THREAD.to_owned())
        .spawn(move || {
            let frame = Frame::read_from(&mut input);
            sender.send((frame, input)).ok();
        })?;
    let (frame, input) = received
        .recv_timeout(wait)
        .map_err(|_| MuxError::TimedOut)?;
    match frame? {
        Some(Frame {
            body: Body::Hello { protocol, version },
            ..
        }) if protocol == PROTOCOL_VERSION => Ok((input, version)),
        Some(Frame {
            body: Body::Hello { protocol, .. },
            ..
        }) => Err(MuxError::Incompatible(protocol)),
        Some(_) => Err(MuxError::Protocol),
        None => Err(MuxError::Closed),
    }
}

fn dispatch(session: &Arc<SessionShared>, input: &mut impl Read) {
    loop {
        let frame = match Frame::read_from(input) {
            Ok(Some(frame)) => frame,
            Ok(None) => return,
            Err(error) => {
                tracing::debug!(%error, "multiplexer stream failed");
                return;
            }
        };
        let id = frame.channel;
        let handled = match frame.body {
            Body::Open(request) => accept_open(session, id, request),
            Body::Hello { .. } => Err(MuxError::Protocol),
            body => route(session, id, body),
        };
        if let Err(error) = handled {
            tracing::warn!(%error, channel = id, "multiplexer peer broke the protocol");
            return;
        }
    }
}

fn route(session: &SessionShared, id: u32, body: Body) -> Result<(), MuxError> {
    if session.settle_abandoned(id, &body) {
        return Ok(());
    }
    let Some(shared) = session.find(id) else {
        return Ok(());
    };
    if matches!(body, Body::Close) {
        session.forget(id);
    }
    shared.receive(body)
}

fn accept_open(
    session: &Arc<SessionShared>,
    id: u32,
    request: OpenRequest,
) -> Result<(), MuxError> {
    if id == CONTROL_CHANNEL || !session.side.peer().owns(id) {
        return Err(MuxError::Protocol);
    }
    let handler = lock(&session.incoming).clone();
    let Some(handler) = handler else {
        refuse(session, id);
        return Ok(());
    };
    let shared = session.register(id).ok_or(MuxError::Protocol)?;
    let channel = Channel::new(Arc::clone(session), shared);
    if handler.send(Incoming { channel, request }).is_err() {
        session.forget(id);
        refuse(session, id);
    }
    Ok(())
}

fn refuse(session: &SessionShared, id: u32) {
    let refused = Frame::new(
        id,
        Body::Refused {
            reason: super::NOT_ACCEPTING.to_owned(),
        },
    );
    session.send(&refused).ok();
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{BufReader, BufWriter};
    use std::net::{TcpListener, TcpStream};

    pub(crate) type End = (Session, mpsc::Receiver<Incoming>);

    pub(crate) fn pair() -> Result<(End, End), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let client = TcpStream::connect(listener.local_addr()?)?;
        let (host, _) = listener.accept()?;
        let wait = Duration::from_secs(5);
        let host_reader = BufReader::new(host.try_clone()?);
        let host_closer = host.try_clone()?;
        let host_writer = BufWriter::new(host);
        let hosting = thread::spawn(move || {
            Session::start(
                host_reader,
                host_writer,
                Side::Host,
                "host",
                wait,
                Some(Box::new(move || {
                    host_closer.shutdown(std::net::Shutdown::Both).ok();
                })),
            )
        });
        let client_closer = client.try_clone()?;
        let (client_session, client_incoming, host_version) = Session::start(
            BufReader::new(client.try_clone()?),
            BufWriter::new(client),
            Side::Client,
            "client",
            wait,
            Some(Box::new(move || {
                client_closer.shutdown(std::net::Shutdown::Both).ok();
            })),
        )?;
        let (host_session, host_incoming, client_version) =
            hosting.join().map_err(|_| "host thread panicked")??;
        assert_eq!(host_version, "host");
        assert_eq!(client_version, "client");
        Ok((
            (client_session, client_incoming),
            (host_session, host_incoming),
        ))
    }

    #[test]
    fn an_unanswered_open_is_refused_once_the_host_stops_listening(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ((client, _), (host, incoming)) = pair()?;
        drop(incoming);
        let refused = client.open(OpenRequest::Sftp, Duration::from_secs(5));
        assert!(matches!(refused, Err(MuxError::Refused(_))));
        host.close();
        Ok(())
    }

    #[test]
    fn ids_belong_to_the_side_that_opens_them() {
        assert!(Side::Client.owns(2));
        assert!(!Side::Client.owns(1));
        assert!(Side::Host.owns(3));
        assert!(!Side::Host.owns(4));
    }

    #[test]
    fn an_open_that_times_out_is_killed_and_a_late_answer_closed(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ((client, _), (_host, incoming)) = pair()?;
        let opened = client.open(OpenRequest::Sftp, Duration::from_millis(50));
        assert!(matches!(opened, Err(MuxError::TimedOut)));
        let late = incoming.recv_timeout(Duration::from_secs(5))?;
        late.channel.accept(None)?;
        let killed = late.channel.wait_for_kill();
        assert!(killed);
        Ok(())
    }

    #[test]
    fn closing_one_end_ends_the_other() -> Result<(), Box<dyn std::error::Error>> {
        let ((client, _), (host, _)) = pair()?;
        host.close();
        let opened = client.open(OpenRequest::Listen, Duration::from_secs(5));
        assert!(opened.is_err());
        Ok(())
    }
}
