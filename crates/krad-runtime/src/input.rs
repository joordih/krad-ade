use futures::channel::{mpsc, oneshot};
use krad_protocol::{Command, Feed, Reply, RequestId, Scope, Snapshot};

#[derive(Debug)]
pub(crate) enum Input {
    Command {
        id: RequestId,
        command: Command,
        reply: oneshot::Sender<Reply>,
    },
    Subscribe(mpsc::Sender<Feed>),
    Snapshot {
        scope: Scope,
        reply: oneshot::Sender<Snapshot>,
    },
    Tick,
}
