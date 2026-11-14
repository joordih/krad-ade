use futures::{stream, FutureExt as _, StreamExt as _};
use krad_client::{Answer, ClientError, Feeds, RuntimeClient};
use krad_protocol::{
    Command, Feed, Hello, ProtocolVersion, Reply, RequestId, RuntimeRun, Scope, Seq, Snapshot,
    Welcome,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

const BUILD: &str = "recorded";
const RUN: RuntimeRun = RuntimeRun(7);

fn seq(feed: &Feed) -> Seq {
    match feed {
        Feed::Snapshot(snapshot) => snapshot.seq,
        Feed::Event(event) => event.seq,
    }
}

#[derive(Debug, Default)]
pub(crate) struct RecordedClient {
    feeds: Mutex<Vec<Feed>>,
    stopped: AtomicBool,
    sent: Mutex<Vec<(RequestId, Command)>>,
    requests: AtomicU64,
}

impl RecordedClient {
    pub(crate) fn new(feeds: Vec<Feed>) -> Arc<Self> {
        Arc::new(Self {
            feeds: Mutex::new(feeds),
            ..Self::default()
        })
    }

    pub(crate) fn record(&self, feed: Feed) {
        self.recorded().push(feed);
    }

    pub(crate) fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }

    fn recorded(&self) -> MutexGuard<'_, Vec<Feed>> {
        self.feeds.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn sent(&self) -> Vec<Command> {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(_, command)| command.clone())
            .collect()
    }
}

impl RuntimeClient for RecordedClient {
    fn hello(&self, _hello: Hello) -> Answer<Welcome> {
        if self.stopped.load(Ordering::Relaxed) {
            return futures::future::ready(Err(ClientError::Stopped)).boxed();
        }
        let welcome = Welcome {
            protocol: ProtocolVersion::CURRENT,
            build: BUILD.to_owned(),
            run: RUN,
            permissions: None,
            first_request: None,
        };
        futures::future::ready(Ok(welcome)).boxed()
    }

    fn next_request(&self) -> RequestId {
        RequestId(self.requests.fetch_add(1, Ordering::Relaxed) + 1)
    }

    fn send_as(&self, id: RequestId, command: Command) -> Answer<Reply> {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((id, command));
        futures::future::ready(Ok(Reply::Accepted)).boxed()
    }

    fn subscribe(&self, since: Option<Seq>) -> Feeds {
        let after = |feed: &Feed| since.is_none_or(|since| seq(feed) > since);
        let feeds: Vec<Feed> = self
            .recorded()
            .iter()
            .filter(|feed| after(feed))
            .cloned()
            .collect();
        stream::iter(feeds).boxed()
    }

    fn snapshot(&self, _scope: Scope) -> Answer<Snapshot> {
        let snapshot = self.recorded().iter().find_map(|feed| match feed {
            Feed::Snapshot(snapshot) => Some(snapshot.clone()),
            Feed::Event(_) => None,
        });
        futures::future::ready(snapshot.ok_or(ClientError::Stopped)).boxed()
    }
}
