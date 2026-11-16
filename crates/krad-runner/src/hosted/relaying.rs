use krad_domain::{CloudRunnerId, PairingPermission, PairingPermissions};
use krad_protocol::{read_frame, write_frame, ErrorCode, Frame, Message, Reply};
use krad_runtime::{RunnerLine, RuntimeHandle};
use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Write as _};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread;

use super::outbox::Outbox;
use super::permits::needed_for_command;

const RELAY_CAPACITY: usize = 256;
const RELAY_THREAD: &str = "krad-runner-relay";
const RELAY_READER_THREAD: &str = "krad-runner-relay-reader";
const UNREACHED: &str = "this computer's runtime could not reach that cloud runner";
const NOT_REACHED: &str = "reach the cloud runner with a hello first";
const CLOSED: &str = "the cloud runner's line closed";
const NOT_ALLOWED: &str = "this pairing does not allow that";
const NOT_RELAYED: &str = "pairing and delegations are not relayed to a cloud runner";

#[derive(Default)]
pub(crate) struct Relays {
    lines: HashMap<CloudRunnerId, SyncSender<Frame>>,
}

fn via(runner: CloudRunnerId, message: Message) -> Message {
    Message::ViaRunner {
        runner,
        message: Box::new(message),
    }
}

fn refusal(code: ErrorCode, message: &str) -> Message {
    Message::Refused {
        code,
        message: message.to_owned(),
    }
}

fn shared(held: &PairingPermissions, offered: Option<PairingPermissions>) -> PairingPermissions {
    let offered = offered.unwrap_or_default();
    PairingPermission::ALL
        .into_iter()
        .filter(|permission| held.allows(*permission) && offered.allows(*permission))
        .collect()
}

fn read_from_runner(
    line: (std::process::ChildStdout, CloudRunnerId),
    held: &PairingPermissions,
    outbox: &Outbox,
) {
    let (input, runner) = line;
    let mut input = BufReader::new(input);
    loop {
        let sent = match read_frame(&mut input) {
            Ok(Some(Frame::Message(message))) => {
                let message = match *message {
                    Message::Welcome(mut welcome) => {
                        welcome.permissions = Some(shared(held, welcome.permissions.take()));
                        Message::Welcome(welcome)
                    }
                    other => other,
                };
                outbox.send(via(runner, message))
            }
            Ok(Some(Frame::Terminal(bytes))) => {
                outbox.send_frame(Frame::RunnerTerminal { runner, bytes })
            }
            Ok(Some(Frame::RunnerTerminal { .. })) => true,
            Ok(None) | Err(_) => {
                outbox.send(via(runner, refusal(ErrorCode::Unavailable, CLOSED)));
                return;
            }
        };
        if !sent {
            return;
        }
    }
}

fn relay(
    handle: &RuntimeHandle,
    (runner, held): (CloudRunnerId, PairingPermissions),
    outbox: &Outbox,
    frames: &Receiver<Frame>,
) {
    let reached = futures::executor::block_on(handle.connect_runner(runner));
    let Ok(Ok(RunnerLine {
        input,
        output,
        hello,
        stop,
    })) = reached
    else {
        outbox.send(via(runner, refusal(ErrorCode::Unavailable, UNREACHED)));
        return;
    };
    let mut output = BufWriter::new(output);
    let greeted = write_frame(
        &mut output,
        &Frame::Message(Box::new(Message::Hello(hello))),
    )
    .map_err(|error| error.to_string())
    .and_then(|()| output.flush().map_err(|error| error.to_string()));
    if let Err(error) = greeted {
        tracing::debug!(%error, "a relayed runner line did not take its hello");
        outbox.send(via(runner, refusal(ErrorCode::Unavailable, UNREACHED)));
        stop.stop();
        return;
    }
    let reading = outbox.clone();
    let spawned = thread::Builder::new()
        .name(RELAY_READER_THREAD.to_owned())
        .spawn(move || read_from_runner((input, runner), &held, &reading));
    if spawned.is_err() {
        stop.stop();
        return;
    }
    for frame in frames {
        let written = write_frame(&mut output, &frame)
            .map_err(|error| error.to_string())
            .and_then(|()| output.flush().map_err(|error| error.to_string()));
        if let Err(error) = written {
            tracing::debug!(%error, "a relayed runner line stopped taking frames");
            break;
        }
    }
    stop.stop();
}

impl Relays {
    pub(crate) fn take(
        &mut self,
        handle: &RuntimeHandle,
        (held, outbox): (&PairingPermissions, &Outbox),
        runner: CloudRunnerId,
        message: Message,
    ) {
        match message {
            Message::Hello(_) => self.open(handle, (runner, held.clone()), outbox),
            Message::Command { id, command } => {
                if !held.allows(needed_for_command(&command)) {
                    let reply = Reply::Refused {
                        code: ErrorCode::Forbidden,
                        message: NOT_ALLOWED.to_owned(),
                    };
                    outbox.send(via(runner, Message::Reply { id, reply }));
                    return;
                }
                self.forward(runner, Message::Command { id, command }, outbox);
            }
            message @ (Message::Subscribe { .. } | Message::AskSnapshot { .. }) => {
                if !held.allows(PairingPermission::Read) {
                    outbox.send(via(runner, refusal(ErrorCode::Forbidden, NOT_ALLOWED)));
                    return;
                }
                self.forward(runner, message, outbox);
            }
            _ => {
                outbox.send(via(runner, refusal(ErrorCode::Unsupported, NOT_RELAYED)));
            }
        }
    }

    fn open(
        &mut self,
        handle: &RuntimeHandle,
        (runner, held): (CloudRunnerId, PairingPermissions),
        outbox: &Outbox,
    ) {
        let (frames, waiting) = sync_channel(RELAY_CAPACITY);
        self.lines.insert(runner, frames);
        let (handle, outbox) = (handle.clone(), outbox.clone());
        let spawned = thread::Builder::new()
            .name(RELAY_THREAD.to_owned())
            .spawn(move || relay(&handle, (runner, held), &outbox, &waiting));
        if let Err(error) = spawned {
            tracing::warn!(%error, "a cloud runner could not be relayed");
            self.lines.remove(&runner);
        }
    }

    fn forward(&mut self, runner: CloudRunnerId, message: Message, outbox: &Outbox) {
        let Some(line) = self.lines.get(&runner) else {
            outbox.send(via(runner, refusal(ErrorCode::NotFound, NOT_REACHED)));
            return;
        };
        if line.send(Frame::Message(Box::new(message))).is_err() {
            self.lines.remove(&runner);
            outbox.send(via(runner, refusal(ErrorCode::Unavailable, CLOSED)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relayed_client_keeps_only_the_permissions_both_sides_hold() {
        let held: PairingPermissions = [PairingPermission::Read, PairingPermission::Answer]
            .into_iter()
            .collect();
        let kept = shared(&held, Some(PairingPermissions::all()));
        assert_eq!(kept, held);
        assert_eq!(shared(&held, None), PairingPermissions::default());
    }
}
