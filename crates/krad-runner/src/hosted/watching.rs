use futures::FutureExt as _;
use krad_protocol::{AgentRef, ErrorCode, Message, Reply, RequestId, TerminalBytes};
use krad_runtime::{RuntimeHandle, TerminalWatch};
use std::thread;

use super::connection::refused;
use super::outbox::{Answers, Outbox};

const WATCH_THREAD: &str = "krad-runner-terminal";
const NO_TERMINAL: &str = "this runtime runs no terminal of that name";
const NO_THREAD: &str = "the terminal could not be watched";
const STOPPED: &str = "the runtime has stopped";

fn forward(terminal: &AgentRef, watch: TerminalWatch, outbox: &Outbox) {
    let id = terminal.terminal;
    let replay = (!watch.replay.is_empty()).then_some(watch.replay);
    for bytes in replay.into_iter().chain(watch.bytes) {
        if !outbox.send_terminal(TerminalBytes {
            terminal: id,
            bytes,
        }) {
            return;
        }
    }
}

fn watched(terminal: AgentRef, watch: Option<TerminalWatch>, outbox: Outbox) -> Reply {
    let Some(watch) = watch else {
        return refused(ErrorCode::NotFound, NO_TERMINAL);
    };
    let spawned = thread::Builder::new()
        .name(WATCH_THREAD.to_owned())
        .spawn(move || forward(&terminal, watch, &outbox));
    match spawned {
        Ok(_) => Reply::Accepted,
        Err(_) => refused(ErrorCode::Unavailable, NO_THREAD),
    }
}

pub(super) fn watch_terminal(
    handle: &RuntimeHandle,
    (outbox, answers): (&Outbox, &Answers),
    id: RequestId,
    terminal: AgentRef,
) {
    let pending = handle.watch_terminal(terminal.clone());
    let outbox = outbox.clone();
    answers.wait_for(
        pending
            .map(move |watch| {
                let reply = match watch {
                    Ok(watch) => watched(terminal, watch, outbox),
                    Err(_) => refused(ErrorCode::Unavailable, STOPPED),
                };
                Message::Reply { id, reply }
            })
            .boxed(),
    );
}
