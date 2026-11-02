use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::session::SessionShared;
use super::wire::{Body, Frame, Stream, DATA_CHUNK_BYTES, INITIAL_WINDOW_BYTES};
use super::MuxError;

mod streams;

pub(crate) use streams::{ChannelReader, ChannelWriter};

const WINDOW_REFILL_BYTES: u32 = INITIAL_WINDOW_BYTES / 2;

struct Inbound {
    chunks: VecDeque<Vec<u8>>,
    ended: bool,
    unacknowledged: u32,
    granted: u32,
}

impl Default for Inbound {
    fn default() -> Self {
        Self {
            chunks: VecDeque::new(),
            ended: false,
            unacknowledged: 0,
            granted: INITIAL_WINDOW_BYTES,
        }
    }
}

enum Opening {
    Waiting,
    Opened(Option<u16>),
    Refused(String),
}

struct State {
    opening: Opening,
    inbound: [Inbound; 2],
    send_window: [u32; 2],
    exited: bool,
    code: Option<i32>,
    kill_requested: bool,
    closed: bool,
}

pub(crate) struct ChannelShared {
    id: u32,
    state: Mutex<State>,
    changed: Condvar,
}

impl ChannelShared {
    pub(crate) fn new(id: u32) -> Self {
        Self {
            id,
            state: Mutex::new(State {
                opening: Opening::Waiting,
                inbound: [Inbound::default(), Inbound::default()],
                send_window: [INITIAL_WINDOW_BYTES; 2],
                exited: false,
                code: None,
                kill_requested: false,
                closed: false,
            }),
            changed: Condvar::new(),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait<'a>(&self, guard: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        self.changed
            .wait(guard)
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn peer_closed(&self) {
        let mut state = self.state();
        state.closed = true;
        for inbound in &mut state.inbound {
            inbound.ended = true;
        }
        if matches!(state.opening, Opening::Waiting) {
            state.opening = Opening::Refused(super::SESSION_ENDED.to_owned());
        }
        drop(state);
        self.changed.notify_all();
    }

    pub(crate) fn receive(&self, body: Body) -> Result<(), MuxError> {
        let mut state = self.state();
        match body {
            Body::Opened { port } => state.opening = Opening::Opened(port),
            Body::Refused { reason } => {
                state.opening = Opening::Refused(reason);
                state.closed = true;
            }
            Body::Data { stream, bytes } => {
                let inbound = &mut state.inbound[stream.index()];
                let len = u32::try_from(bytes.len()).map_err(|_| MuxError::Protocol)?;
                inbound.granted = inbound.granted.checked_sub(len).ok_or(MuxError::Protocol)?;
                if !bytes.is_empty() {
                    inbound.chunks.push_back(bytes);
                }
            }
            Body::Eof { stream } => state.inbound[stream.index()].ended = true,
            Body::Window { stream, bytes } => {
                let window = &mut state.send_window[stream.index()];
                *window = window.saturating_add(bytes);
            }
            Body::Exit { code } => {
                state.exited = true;
                state.code = code;
            }
            Body::Kill => state.kill_requested = true,
            Body::Close => {
                drop(state);
                self.peer_closed();
                return Ok(());
            }
            Body::Hello { .. } | Body::Open(_) => {}
        }
        drop(state);
        self.changed.notify_all();
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct Channel {
    session: Arc<SessionShared>,
    shared: Arc<ChannelShared>,
}

impl Channel {
    pub(crate) const fn new(session: Arc<SessionShared>, shared: Arc<ChannelShared>) -> Self {
        Self { session, shared }
    }

    pub(crate) fn id(&self) -> u32 {
        self.shared.id
    }

    fn send(&self, body: Body) -> Result<(), MuxError> {
        self.session.send(&Frame::new(self.shared.id, body))
    }

    pub(crate) fn wait_opened(&self, wait: Duration) -> Result<Option<u16>, MuxError> {
        let deadline = Instant::now() + wait;
        let mut state = self.shared.state();
        loop {
            match &state.opening {
                Opening::Opened(port) => return Ok(*port),
                Opening::Refused(reason) => return Err(MuxError::Refused(reason.clone())),
                Opening::Waiting => {}
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(MuxError::TimedOut);
            }
            state = self
                .shared
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    pub(crate) fn accept(&self, port: Option<u16>) -> Result<(), MuxError> {
        self.shared.state().opening = Opening::Opened(port);
        self.send(Body::Opened { port })
    }

    pub(crate) fn refuse(&self, reason: &str) {
        self.send(Body::Refused {
            reason: reason.to_owned(),
        })
        .ok();
        self.session.forget(self.shared.id);
    }

    pub(crate) fn write_all(&self, stream: Stream, mut bytes: &[u8]) -> Result<(), MuxError> {
        while !bytes.is_empty() {
            let allowed = {
                let mut state = self.shared.state();
                loop {
                    if state.closed {
                        return Err(MuxError::Closed);
                    }
                    let window = state.send_window[stream.index()];
                    if window > 0 {
                        let take = usize::try_from(window)
                            .unwrap_or(usize::MAX)
                            .min(DATA_CHUNK_BYTES)
                            .min(bytes.len());
                        state.send_window[stream.index()] -=
                            u32::try_from(take).map_err(|_| MuxError::Protocol)?;
                        break take;
                    }
                    state = self.shared.wait(state);
                }
            };
            let (chunk, rest) = bytes.split_at(allowed);
            self.send(Body::Data {
                stream,
                bytes: chunk.to_vec(),
            })?;
            bytes = rest;
        }
        Ok(())
    }

    pub(crate) fn end(&self, stream: Stream) {
        self.send(Body::Eof { stream }).ok();
    }

    pub(crate) fn read(&self, stream: Stream, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut state = self.shared.state();
        loop {
            let inbound = &mut state.inbound[stream.index()];
            if let Some(chunk) = inbound.chunks.front_mut() {
                let count = chunk.len().min(buf.len());
                buf[..count].copy_from_slice(&chunk[..count]);
                chunk.drain(..count);
                if chunk.is_empty() {
                    inbound.chunks.pop_front();
                }
                inbound.unacknowledged = inbound
                    .unacknowledged
                    .saturating_add(u32::try_from(count).unwrap_or(u32::MAX));
                let refill = (inbound.unacknowledged >= WINDOW_REFILL_BYTES)
                    .then(|| std::mem::take(&mut inbound.unacknowledged));
                if let Some(bytes) = refill {
                    inbound.granted = inbound.granted.saturating_add(bytes);
                }
                drop(state);
                if let Some(bytes) = refill {
                    self.send(Body::Window { stream, bytes }).ok();
                }
                return Ok(count);
            }
            if inbound.ended || state.closed {
                return Ok(0);
            }
            state = self.shared.wait(state);
        }
    }

    pub(crate) fn exit(&self, code: Option<i32>) -> Result<(), MuxError> {
        self.send(Body::Exit { code })
    }

    pub(crate) fn wait_exit(&self, wait: Option<Duration>) -> Result<Option<i32>, MuxError> {
        let deadline = wait.map(|wait| Instant::now() + wait);
        let mut state = self.shared.state();
        loop {
            if state.exited {
                return Ok(state.code);
            }
            if state.closed {
                return Err(MuxError::Closed);
            }
            state = match deadline {
                None => self.shared.wait(state),
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(MuxError::TimedOut);
                    }
                    self.shared
                        .changed
                        .wait_timeout(state, left)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
            };
        }
    }

    pub(crate) fn kill(&self) {
        self.send(Body::Kill).ok();
    }

    pub(crate) fn wait_for_kill(&self) -> bool {
        let mut state = self.shared.state();
        loop {
            if state.kill_requested {
                return true;
            }
            if state.closed {
                return false;
            }
            state = self.shared.wait(state);
        }
    }

    pub(crate) fn close(&self) {
        let already = {
            let mut state = self.shared.state();
            std::mem::replace(&mut state.closed, true)
        };
        self.shared.changed.notify_all();
        if !already {
            self.send(Body::Close).ok();
        }
        self.session.forget(self.shared.id);
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.shared.state().closed || self.session.is_closed()
    }

    pub(crate) fn reader(&self, stream: Stream) -> ChannelReader {
        ChannelReader {
            channel: self.clone(),
            stream,
        }
    }

    pub(crate) fn writer(&self, stream: Stream) -> ChannelWriter {
        ChannelWriter {
            channel: self.clone(),
            stream,
            ended: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::session::tests::pair;
    use super::super::wire::OpenRequest;
    use super::*;
    use std::io::{Read, Write};
    use std::thread;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn data_larger_than_the_window_arrives_whole_and_in_order() -> TestResult {
        let ((client, _), (_host, incoming)) = pair()?;
        let serving = thread::spawn(move || -> Result<Vec<u8>, MuxError> {
            let Ok(request) = incoming.recv() else {
                return Err(MuxError::Closed);
            };
            request.channel.accept(None)?;
            let mut received = Vec::new();
            request
                .channel
                .reader(Stream::Main)
                .read_to_end(&mut received)?;
            request.channel.exit(Some(3))?;
            request.channel.close();
            Ok(received)
        });
        let (channel, _) = client.open(OpenRequest::Sftp, Duration::from_secs(5))?;
        let sent: Vec<u8> = (0..3 * INITIAL_WINDOW_BYTES)
            .map(|at| u8::try_from(at % 251).unwrap_or_default())
            .collect();
        let mut writer = channel.writer(Stream::Main);
        writer.write_all(&sent)?;
        writer.finish();
        assert_eq!(channel.wait_exit(Some(Duration::from_secs(10)))?, Some(3));
        let received = serving.join().map_err(|_| "host thread panicked")??;
        assert_eq!(received, sent);
        Ok(())
    }

    #[test]
    fn data_beyond_the_granted_window_is_a_protocol_error() {
        let shared = ChannelShared::new(2);
        let within = shared.receive(Body::Data {
            stream: Stream::Main,
            bytes: vec![0; usize::try_from(INITIAL_WINDOW_BYTES).unwrap_or_default()],
        });
        assert!(within.is_ok());
        let beyond = shared.receive(Body::Data {
            stream: Stream::Main,
            bytes: vec![0; 1],
        });
        assert!(matches!(beyond, Err(MuxError::Protocol)));
    }

    #[test]
    fn a_kill_reaches_the_other_end_and_closing_wakes_readers() -> TestResult {
        let ((client, _), (_host, incoming)) = pair()?;
        let serving = thread::spawn(move || -> Result<bool, MuxError> {
            let request = incoming.recv().map_err(|_| MuxError::Closed)?;
            request.channel.accept(None)?;
            let killed = request.channel.wait_for_kill();
            request.channel.close();
            Ok(killed)
        });
        let (channel, _) = client.open(OpenRequest::Listen, Duration::from_secs(5))?;
        channel.kill();
        assert!(serving.join().map_err(|_| "host thread panicked")??);
        let mut buf = [0; 8];
        assert_eq!(channel.reader(Stream::Main).read(&mut buf)?, 0);
        Ok(())
    }
}
