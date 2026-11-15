mod inbox;

use futures::channel::{mpsc, oneshot};
use futures::{FutureExt as _, StreamExt as _};
use krad_protocol::{
    read_frame, write_frame, AgentRef, Command, ErrorCode, Frame, Hello, Message, PairingAnswer,
    PairingCommand, Reply, RequestId, Scope, Seq, Snapshot, TerminalCommand, Welcome,
};
use std::io::{BufReader, BufWriter, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use self::inbox::{read_all, Awaiting, Waiting};
use crate::client::{Answer, ClientError, DelegatedFeed, Feeds, RuntimeClient, TerminalBytesFeed};

const OUTBOX_CAPACITY: usize = 256;
const FEED_CAPACITY: usize = 1024;
const TERMINAL_CAPACITY: usize = 256;
const DELEGATION_CAPACITY: usize = 64;
const WRITER_THREAD: &str = "krad-client-runner-writer";
const READER_THREAD: &str = "krad-client-runner-reader";
const NO_WELCOME: &str = "the runner did not welcome the client";
const WELCOME_TOO_SLOW: &str = "the runner did not welcome the client in time";
const CONNECT_LIMIT: Duration = Duration::from_secs(20);
const NO_THREAD: &str = "the runner client could not start its threads";
const FIRST_REQUEST: RequestId = RequestId(1);

pub struct Runner {
    outbox: SyncSender<Message>,
    waiting: Arc<Waiting>,
    next: AtomicU64,
    welcome: Welcome,
}

fn failed(message: &str) -> ClientError {
    ClientError::Failed {
        code: ErrorCode::Unavailable,
        message: message.to_owned(),
    }
}

fn welcome_from(input: &mut impl Read) -> Result<Welcome, ClientError> {
    let Ok(Some(Frame::Message(message))) = read_frame(input) else {
        return Err(failed(NO_WELCOME));
    };
    match *message {
        Message::Welcome(welcome) => Ok(welcome),
        Message::Refused { code, message } => Err(ClientError::Refused { code, message }),
        _ => Err(failed(NO_WELCOME)),
    }
}

fn write_all(output: impl Write, outbox: &Receiver<Message>) {
    let mut output = BufWriter::new(output);
    for message in outbox {
        let frame = Frame::Message(Box::new(message));
        if write_frame(&mut output, &frame).is_err() || output.flush().is_err() {
            return;
        }
    }
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) -> Result<(), ClientError> {
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(work)
        .map(drop)
        .map_err(|_| failed(NO_THREAD))
}

impl Runner {
    /// # Errors
    ///
    /// Fails when the runner refuses the client, answers with something else than a welcome,
    /// or the connection fails before it does.
    pub fn connect(
        input: impl Read + Send + 'static,
        output: impl Write + Send + 'static,
        hello: Hello,
    ) -> Result<Self, ClientError> {
        Self::connect_within(input, output, hello, CONNECT_LIMIT)
    }

    /// # Errors
    ///
    /// Fails as `connect` does, and when the runner does not welcome the client within `limit`.
    pub fn connect_within(
        input: impl Read + Send + 'static,
        output: impl Write + Send + 'static,
        hello: Hello,
        limit: Duration,
    ) -> Result<Self, ClientError> {
        let mut output = BufWriter::new(output);
        write_frame(
            &mut output,
            &Frame::Message(Box::new(Message::Hello(hello))),
        )
        .map_err(|_| failed(NO_WELCOME))?;
        output.flush().map_err(|_| failed(NO_WELCOME))?;
        let waiting = Arc::new(Waiting::default());
        let reading = Arc::clone(&waiting);
        let (welcomed, welcome) = sync_channel(1);
        spawn(READER_THREAD, move || {
            let mut input = BufReader::new(input);
            let greeted = welcome_from(&mut input);
            let go_on = greeted.is_ok();
            if welcomed.send(greeted).is_ok() && go_on {
                read_all(&mut input, &reading);
            }
        })?;
        let welcome = match welcome.recv_timeout(limit) {
            Ok(welcome) => welcome?,
            Err(RecvTimeoutError::Timeout) => return Err(failed(WELCOME_TOO_SLOW)),
            Err(RecvTimeoutError::Disconnected) => return Err(failed(NO_WELCOME)),
        };
        let (outbox, outgoing) = sync_channel(OUTBOX_CAPACITY);
        spawn(WRITER_THREAD, move || write_all(output, &outgoing))?;
        let first = welcome.first_request.unwrap_or(FIRST_REQUEST);
        Ok(Self {
            outbox,
            waiting,
            next: AtomicU64::new(first.0),
            welcome,
        })
    }

    #[must_use]
    pub const fn welcome(&self) -> &Welcome {
        &self.welcome
    }

    fn ask<T: Send + 'static>(
        &self,
        id: RequestId,
        awaiting: Awaiting,
        message: Message,
        answer: oneshot::Receiver<Result<T, Reply>>,
    ) -> Answer<T> {
        self.waiting.wait(id, awaiting);
        if self.outbox.send(message).is_err() {
            self.waiting.forget(id);
        }
        answer
            .map(|answer| match answer {
                Ok(Ok(answer)) => Ok(answer),
                Ok(Err(reply)) => Err(ClientError::from(reply)),
                Err(oneshot::Canceled) => Err(ClientError::Stopped),
            })
            .boxed()
    }

    pub fn take_delegations(&self) -> DelegatedFeed {
        let (sender, delegated) = mpsc::channel(DELEGATION_CAPACITY);
        self.waiting.delegate_to(sender);
        if self.outbox.send(Message::TakeDelegations).is_err() {
            tracing::debug!("the runner connection closed before taking delegations");
        }
        delegated.boxed()
    }

    pub fn pairing(&self, command: PairingCommand) -> Answer<PairingAnswer> {
        let id = self.next_request();
        let (sender, answer) = oneshot::channel();
        let message = Message::Pairing { id, command };
        self.ask(id, Awaiting::Pairing(sender), message, answer)
    }
}

impl RuntimeClient for Runner {
    fn hello(&self, _hello: Hello) -> Answer<Welcome> {
        futures::future::ready(Ok(self.welcome.clone())).boxed()
    }

    fn next_request(&self) -> RequestId {
        RequestId(self.next.fetch_add(1, Ordering::Relaxed))
    }

    fn send_as(&self, id: RequestId, command: Command) -> Answer<Reply> {
        let (sender, answer) = oneshot::channel();
        let message = Message::Command { id, command };
        self.ask(id, Awaiting::Reply(sender), message, answer)
    }

    fn subscribe(&self, since: Option<Seq>) -> Feeds {
        let (sender, feeds) = mpsc::channel(FEED_CAPACITY);
        self.waiting.follow(sender);
        if self.outbox.send(Message::Subscribe { since }).is_err() {
            self.waiting.stop_following();
        }
        feeds.boxed()
    }

    fn watch_terminal(&self, terminal: AgentRef) -> (Answer<Reply>, TerminalBytesFeed) {
        let (sink, bytes) = mpsc::channel(TERMINAL_CAPACITY);
        self.waiting.watch(terminal.terminal, sink);
        let watch = Command::Terminal(TerminalCommand::Watch { terminal });
        (self.send(watch), bytes.boxed())
    }

    fn snapshot(&self, scope: Scope) -> Answer<Snapshot> {
        let id = self.next_request();
        let (sender, answer) = oneshot::channel();
        let message = Message::AskSnapshot { id, scope };
        self.ask(id, Awaiting::Snapshot(sender), message, answer)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use std::time::Instant;

    const BUILD: &str = "0.0.0";
    const SHORT: Duration = Duration::from_millis(50);

    #[test]
    fn a_runner_that_never_welcomes_the_client_is_given_up_on() -> std::io::Result<()> {
        let (ours, theirs) = UnixStream::pair()?;
        let started = Instant::now();
        let connected = Runner::connect_within(ours.try_clone()?, ours, Hello::new(BUILD), SHORT);
        assert!(matches!(
            connected,
            Err(ClientError::Failed {
                code: ErrorCode::Unavailable,
                ..
            })
        ));
        assert!(started.elapsed() < CONNECT_LIMIT);
        drop(theirs);
        Ok(())
    }
}
