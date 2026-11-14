use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{FutureExt as _, StreamExt as _};
use krad_protocol::{
    AgentRef, Command, ErrorCode, Feed, Hello, Reply, RequestId, Scope, Seq, Snapshot, Welcome,
};

pub type Answer<T> = BoxFuture<'static, Result<T, ClientError>>;
pub type Feeds = BoxStream<'static, Feed>;
pub type TerminalBytesFeed = BoxStream<'static, Vec<u8>>;

const NO_TERMINAL_BYTES: &str = "this client cannot watch a terminal's bytes";

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum ClientError {
    #[error("the runtime has stopped")]
    Stopped,
    #[error("the runtime refused the client: {message}")]
    Refused { code: ErrorCode, message: String },
    #[error("{message}")]
    Failed { code: ErrorCode, message: String },
}

impl From<Reply> for ClientError {
    fn from(reply: Reply) -> Self {
        match reply {
            Reply::Refused { code, message } => Self::Refused { code, message },
            Reply::Accepted => Self::Refused {
                code: ErrorCode::Unknown,
                message: String::new(),
            },
        }
    }
}

pub trait RuntimeClient: Send + Sync {
    fn hello(&self, hello: Hello) -> Answer<Welcome>;

    fn next_request(&self) -> RequestId;

    fn send_as(&self, id: RequestId, command: Command) -> Answer<Reply>;

    fn subscribe(&self, since: Option<Seq>) -> Feeds;

    fn snapshot(&self, scope: Scope) -> Answer<Snapshot>;

    fn send(&self, command: Command) -> Answer<Reply> {
        self.send_as(self.next_request(), command)
    }

    fn post(&self, command: Command) {
        self.post_as(self.next_request(), command);
    }

    fn post_as(&self, id: RequestId, command: Command) {
        drop(self.send_as(id, command));
    }

    fn watch_terminal(&self, _terminal: AgentRef) -> (Answer<Reply>, TerminalBytesFeed) {
        let refused = Reply::Refused {
            code: ErrorCode::Unsupported,
            message: NO_TERMINAL_BYTES.to_owned(),
        };
        (
            futures::future::ready(Ok(refused)).boxed(),
            futures::stream::empty().boxed(),
        )
    }
}
