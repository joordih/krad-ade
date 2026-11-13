use futures::{FutureExt as _, StreamExt as _};
use krad_protocol::{Command, Hello, Reply, RequestId, Scope, Seq, Snapshot, Welcome};
use krad_runtime::{RuntimeHandle, RuntimeStopped};

use crate::client::{Answer, ClientError, Feeds, RuntimeClient};

impl From<RuntimeStopped> for ClientError {
    fn from(RuntimeStopped: RuntimeStopped) -> Self {
        Self::Stopped
    }
}

#[derive(Clone, Debug)]
pub struct InProcess {
    handle: RuntimeHandle,
}

impl InProcess {
    #[must_use]
    pub const fn new(handle: RuntimeHandle) -> Self {
        Self { handle }
    }

    #[must_use]
    pub const fn handle(&self) -> &RuntimeHandle {
        &self.handle
    }
}

impl RuntimeClient for InProcess {
    fn hello(&self, hello: Hello) -> Answer<Welcome> {
        let welcome = self.handle.hello(&hello).map_err(ClientError::from);
        futures::future::ready(welcome).boxed()
    }

    fn next_request(&self) -> RequestId {
        self.handle.next_request()
    }

    fn send_as(&self, id: RequestId, command: Command) -> Answer<Reply> {
        self.handle
            .send(id, command)
            .map(|reply| reply.map_err(ClientError::from))
            .boxed()
    }

    fn subscribe(&self, since: Option<Seq>) -> Feeds {
        match since {
            Some(since) => self.handle.resume(since).boxed(),
            None => self.handle.subscribe().boxed(),
        }
    }

    fn snapshot(&self, scope: Scope) -> Answer<Snapshot> {
        self.handle
            .snapshot(scope)
            .map(|snapshot| snapshot.map_err(ClientError::from))
            .boxed()
    }
}
