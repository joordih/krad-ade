use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use super::wire::{Message, Packet, PacketType};
use super::SftpError;

const READER_THREAD: &str = "krad-ade-sftp";

type Writer = Box<dyn Write + Send>;

pub(super) struct Channel {
    writer: Mutex<Writer>,
    pending: Arc<Mutex<HashMap<u32, Sender<Message>>>>,
    closed: Arc<AtomicBool>,
    next_id: AtomicU32,
    pub(super) timeout: Duration,
}

pub(super) struct Pending<'a> {
    id: u32,
    replies: Receiver<Message>,
    channel: &'a Channel,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn broken() -> SftpError {
    SftpError::Io(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "the SFTP channel closed",
    ))
}

impl Channel {
    pub(super) fn start(
        mut input: impl Read + Send + 'static,
        writer: impl Write + Send + 'static,
        timeout: Duration,
    ) -> io::Result<Self> {
        let pending: Arc<Mutex<HashMap<u32, Sender<Message>>>> = Arc::default();
        let closed = Arc::new(AtomicBool::new(false));
        let reader_pending = Arc::clone(&pending);
        let reader_closed = Arc::clone(&closed);
        thread::Builder::new()
            .name(READER_THREAD.to_owned())
            .spawn(move || {
                while let Ok(reply) = Message::receive(&mut input) {
                    if let Some(waiter) = lock(&reader_pending).remove(&reply.id) {
                        waiter.send(reply).ok();
                    }
                }
                reader_closed.store(true, Ordering::Release);
                lock(&reader_pending).clear();
                tracing::debug!("sftp channel reader stopped");
            })?;
        Ok(Self {
            writer: Mutex::new(Box::new(writer)),
            pending,
            closed,
            next_id: AtomicU32::new(1),
            timeout,
        })
    }

    pub(super) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    pub(super) fn send(
        &self,
        kind: PacketType,
        fill: impl FnOnce(&mut Packet) -> io::Result<()>,
    ) -> Result<Pending<'_>, SftpError> {
        if self.is_closed() {
            return Err(broken());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut packet = Packet::request(kind, id);
        fill(&mut packet)?;
        let (sender, replies) = mpsc::channel();
        lock(&self.pending).insert(id, sender);
        let pending = Pending {
            id,
            replies,
            channel: self,
        };
        if let Err(error) = packet.send(&mut *lock(&self.writer)) {
            self.closed.store(true, Ordering::Release);
            return Err(SftpError::Io(error));
        }
        Ok(pending)
    }

    pub(super) fn call(
        &self,
        kind: PacketType,
        fill: impl FnOnce(&mut Packet) -> io::Result<()>,
    ) -> Result<Message, SftpError> {
        self.send(kind, fill)?.wait()
    }
}

impl Pending<'_> {
    pub(super) fn wait(self) -> Result<Message, SftpError> {
        match self.replies.recv_timeout(self.channel.timeout) {
            Ok(reply) => Ok(reply),
            Err(RecvTimeoutError::Timeout) => Err(SftpError::TimedOut),
            Err(RecvTimeoutError::Disconnected) => Err(broken()),
        }
    }
}

impl Drop for Pending<'_> {
    fn drop(&mut self) {
        lock(&self.channel.pending).remove(&self.id);
    }
}
