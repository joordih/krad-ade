use krad_domain::Pairing;
use krad_protocol::{read_frame, write_frame, Frame, Message};
use krad_runner::Stream;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message as WsMessage, WebSocket};

use crate::tls::Wire;

const LENGTH_BYTES: usize = 4;
const INBOUND_CAPACITY: usize = 256;
const OUTBOUND_CAPACITY: usize = 256;
const POLL: Duration = Duration::from_millis(8);
const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024 + LENGTH_BYTES;
const PUMP_THREAD: &str = "krad-web-socket";
const FRAME_TOO_LARGE: &str = "a frame is over the web socket's limit";
const CLOSED: &str = "the web socket closed";

struct Inbound {
    frames: Receiver<Vec<u8>>,
    current: Vec<u8>,
    read: usize,
}

struct Shared {
    inbound: Mutex<Inbound>,
    outbound: SyncSender<Vec<u8>>,
    assembling: Mutex<Vec<u8>>,
    closed: AtomicBool,
}

pub struct WebStream {
    shared: Arc<Shared>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn take_frame(assembling: &mut Vec<u8>) -> io::Result<Option<Vec<u8>>> {
    let Some(header) = assembling.first_chunk::<LENGTH_BYTES>() else {
        return Ok(None);
    };
    let length = usize::try_from(u32::from_be_bytes(*header)).map_err(io::Error::other)?;
    if length + LENGTH_BYTES > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, FRAME_TOO_LARGE));
    }
    if assembling.len() < LENGTH_BYTES + length {
        return Ok(None);
    }
    let payload = assembling[LENGTH_BYTES..LENGTH_BYTES + length].to_vec();
    assembling.drain(..LENGTH_BYTES + length);
    Ok(Some(payload))
}

fn prefixed(payload: &[u8]) -> io::Result<Vec<u8>> {
    let length = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, FRAME_TOO_LARGE))?;
    let mut bytes = Vec::with_capacity(LENGTH_BYTES + payload.len());
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

fn with_pairing(bytes: Vec<u8>, pairing: &Pairing) -> Vec<u8> {
    let Ok(Some(Frame::Message(message))) = read_frame(&mut bytes.as_slice()) else {
        return bytes;
    };
    let Message::Hello(mut hello) = *message else {
        return bytes;
    };
    if hello.pairing.is_some() {
        return bytes;
    }
    hello.pairing = Some(pairing.clone());
    let mut paired = Vec::new();
    match write_frame(
        &mut paired,
        &Frame::Message(Box::new(Message::Hello(hello))),
    ) {
        Ok(()) => paired,
        Err(error) => {
            tracing::debug!(%error, "a web hello could not carry its pairing");
            bytes
        }
    }
}

impl Read for WebStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut inbound = lock(&self.shared.inbound);
        if inbound.read == inbound.current.len() {
            match inbound.frames.recv() {
                Ok(frame) => {
                    inbound.current = frame;
                    inbound.read = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let start = inbound.read;
        let count = buf.len().min(inbound.current.len() - start);
        buf[..count].copy_from_slice(&inbound.current[start..start + count]);
        inbound.read += count;
        Ok(count)
    }
}

impl Write for WebStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut assembling = lock(&self.shared.assembling);
        assembling.extend_from_slice(buf);
        while let Some(payload) = take_frame(&mut assembling)? {
            self.shared
                .outbound
                .send(payload)
                .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, CLOSED))?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Stream for WebStream {
    fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            shared: Arc::clone(&self.shared),
        })
    }

    fn close(&self) {
        self.shared.closed.store(true, Ordering::Release);
    }
}

struct Pump {
    socket: WebSocket<Wire>,
    inbound: SyncSender<Vec<u8>>,
    outbound: Receiver<Vec<u8>>,
    shared: Arc<Shared>,
    pairing: Option<Pairing>,
}

enum Turn {
    Again,
    Done,
}

fn is_wait(error: &tungstenite::Error) -> bool {
    matches!(error, tungstenite::Error::Io(io) if matches!(io.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut))
}

impl Pump {
    fn send_pending(&mut self) -> Turn {
        loop {
            match self.outbound.try_recv() {
                Ok(payload) => {
                    if let Err(error) = self.socket.write(WsMessage::Binary(payload.into())) {
                        tracing::debug!(%error, "a web socket stopped taking frames");
                        return Turn::Done;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Turn::Done,
            }
        }
        match self.socket.flush() {
            Err(error) if !is_wait(&error) => {
                tracing::debug!(%error, "a web socket could not flush");
                Turn::Done
            }
            _ => Turn::Again,
        }
    }

    fn deliver(&mut self, payload: &[u8]) -> Turn {
        let Ok(mut bytes) = prefixed(payload) else {
            return Turn::Done;
        };
        if let Some(pairing) = self.pairing.take() {
            bytes = with_pairing(bytes, &pairing);
        }
        match self.inbound.try_send(bytes) {
            Ok(()) => Turn::Again,
            Err(TrySendError::Full(_)) => {
                tracing::debug!("a web client sent faster than the runtime reads");
                Turn::Done
            }
            Err(TrySendError::Disconnected(_)) => Turn::Done,
        }
    }

    fn receive(&mut self) -> Turn {
        match self.socket.read() {
            Ok(WsMessage::Binary(payload)) => self.deliver(&payload),
            Ok(WsMessage::Ping(_) | WsMessage::Pong(_) | WsMessage::Frame(_)) => Turn::Again,
            Ok(WsMessage::Text(_) | WsMessage::Close(_)) => Turn::Done,
            Err(error) if is_wait(&error) => Turn::Again,
            Err(error) => {
                tracing::debug!(%error, "a web socket ended");
                Turn::Done
            }
        }
    }

    fn run(mut self) {
        while !self.shared.closed.load(Ordering::Acquire) {
            if matches!(self.send_pending(), Turn::Done) || matches!(self.receive(), Turn::Done) {
                break;
            }
        }
        if let Err(error) = self.socket.close(None).and_then(|()| self.socket.flush()) {
            tracing::trace!(%error, "a web socket closed without its close frame");
        }
    }
}

pub(crate) fn config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES))
}

pub(crate) fn start(socket: WebSocket<Wire>, pairing: Option<Pairing>) -> io::Result<WebStream> {
    socket.get_ref().set_read_timeout(POLL)?;
    let (inbound, frames) = mpsc::sync_channel(INBOUND_CAPACITY);
    let (outbound, pending) = mpsc::sync_channel(OUTBOUND_CAPACITY);
    let shared = Arc::new(Shared {
        inbound: Mutex::new(Inbound {
            frames,
            current: Vec::new(),
            read: 0,
        }),
        outbound,
        assembling: Mutex::new(Vec::new()),
        closed: AtomicBool::new(false),
    });
    let pump = Pump {
        socket,
        inbound,
        outbound: pending,
        shared: Arc::clone(&shared),
        pairing,
    };
    thread::Builder::new()
        .name(PUMP_THREAD.to_owned())
        .spawn(move || pump.run())?;
    Ok(WebStream { shared })
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_protocol::Hello;

    #[test]
    fn written_bytes_leave_as_whole_frames_without_their_length(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut assembling = prefixed(b"first")?;
        let second = prefixed(b"second")?;
        assembling.extend_from_slice(&second[..3]);
        assert_eq!(take_frame(&mut assembling)?, Some(b"first".to_vec()));
        assert_eq!(take_frame(&mut assembling)?, None);
        assembling.extend_from_slice(&second[3..]);
        assert_eq!(take_frame(&mut assembling)?, Some(b"second".to_vec()));
        assert_eq!(assembling, Vec::<u8>::new());
        Ok(())
    }

    #[test]
    fn the_cookie_pairing_reaches_a_hello_that_has_none() -> Result<(), Box<dyn std::error::Error>>
    {
        let pairing = Pairing {
            install: "web-0123456789".parse()?,
            token: "00112233445566778899aabbccddeeff".parse()?,
        };
        let mut bytes = Vec::new();
        write_frame(
            &mut bytes,
            &Frame::Message(Box::new(Message::Hello(Hello::new("web")))),
        )?;
        let paired = with_pairing(bytes, &pairing);
        let Some(Frame::Message(message)) = read_frame(&mut paired.as_slice())? else {
            return Err("no frame".into());
        };
        let Message::Hello(hello) = *message else {
            return Err("not a hello".into());
        };
        assert_eq!(hello.pairing, Some(pairing));
        Ok(())
    }
}
