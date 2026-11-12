use futures::channel::oneshot;
use krad_protocol::{
    Command, ErrorCode, Event, EventKind, Reply, RequestId, RuntimeCommand, Scope, Seq, Snapshot,
};
use std::io;
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use crate::handle::RuntimeHandle;
use crate::input::Input;
use crate::subscribers::Subscribers;

const THREAD_NAME: &str = "krad-runtime";
const INPUT_CAPACITY: usize = 1024;
const TICK_INTERVAL: Duration = Duration::from_millis(100);
const UNSUPPORTED_COMMAND: &str = "this runtime does not know the command";

#[derive(Debug)]
pub struct Runtime {
    seq: Seq,
    outbox: Vec<EventKind>,
    subscribers: Subscribers,
}

impl Runtime {
    /// # Errors
    ///
    /// Fails when the runtime's thread cannot be spawned.
    #[tracing::instrument(skip_all)]
    pub fn start() -> io::Result<RuntimeHandle> {
        let (inbox, input) = sync_channel(INPUT_CAPACITY);
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || Self::idle().run(&input))?;
        tracing::info!("runtime started");
        Ok(RuntimeHandle::new(inbox))
    }

    fn idle() -> Self {
        Self {
            seq: Seq::default(),
            outbox: Vec::new(),
            subscribers: Subscribers::default(),
        }
    }

    fn run(mut self, input: &Receiver<Input>) {
        let mut next_tick = Instant::now() + TICK_INTERVAL;
        loop {
            let wait = next_tick.saturating_duration_since(Instant::now());
            let mut batch = match input.recv_timeout(wait) {
                Ok(first) => vec![first],
                Err(RecvTimeoutError::Timeout) => Vec::new(),
                Err(RecvTimeoutError::Disconnected) => break,
            };
            batch.extend(input.try_iter());
            if Instant::now() >= next_tick {
                next_tick = Instant::now() + TICK_INTERVAL;
                batch.push(Input::Tick);
            }
            for input in batch {
                self.apply(input);
            }
            self.flush();
        }
        tracing::info!("runtime stopped");
    }

    fn apply(&mut self, input: Input) {
        match input {
            Input::Command { id, command, reply } => {
                let answer = self.execute(id, &command);
                answer_to(reply, answer);
            }
            Input::Subscribe(feed) => self.subscribers.add(feed),
            Input::Snapshot { scope, reply } => answer_to(reply, self.snapshot(scope)),
            Input::Tick => {}
        }
    }

    fn execute(&mut self, id: RequestId, command: &Command) -> Reply {
        match command {
            Command::Runtime(RuntimeCommand::Ping) => {
                self.outbox.push(EventKind::Pong { request: id });
                Reply::Accepted
            }
            Command::Runtime(RuntimeCommand::Unknown) | Command::Unknown => Reply::Refused {
                code: ErrorCode::Unsupported,
                message: UNSUPPORTED_COMMAND.to_owned(),
            },
        }
    }

    fn flush(&mut self) {
        let events: Vec<Event> = self
            .outbox
            .drain(..)
            .map(|kind| {
                self.seq = self.seq.next();
                Event {
                    seq: self.seq,
                    kind,
                }
            })
            .collect();
        let snapshot = self.snapshot(Scope::Runtime);
        self.subscribers.publish(&events, &snapshot);
    }

    fn snapshot(&self, scope: Scope) -> Snapshot {
        Snapshot {
            scope,
            seq: self.seq,
        }
    }
}

fn answer_to<T>(reply: oneshot::Sender<T>, answer: T) {
    if reply.send(answer).is_err() {
        tracing::trace!("a client left before its answer");
    }
}
