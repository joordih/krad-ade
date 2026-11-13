use futures::channel::oneshot;
use krad_protocol::{ErrorCode, Reply};

pub(crate) fn refused(code: ErrorCode, message: &str) -> Reply {
    Reply::Refused {
        code,
        message: message.to_owned(),
    }
}

pub(crate) fn answer_to<T: std::fmt::Debug>(reply: oneshot::Sender<T>, answer: T) {
    if let Err(unheard) = reply.send(answer) {
        tracing::trace!(?unheard, "a client left before its answer");
    }
}
