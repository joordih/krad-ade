use futures::FutureExt as _;
use krad_domain::{InstallId, PairingPermission, PairingPermissions};
use krad_protocol::{
    read_frame, BusCommand, Command, ErrorCode, Frame, Hello, Message, Reply, RequestId, Scope,
    Seq, TerminalCommand, Welcome,
};
use std::io::{BufReader, Read};
use std::sync::Arc;

use super::delegating::{take_delegations, Delegations};
use super::listener::Stream;
use super::outbox::{Answers, Outbox};
use super::pairing_requests::answer_pairing;
use super::permits::needed_for_command;
use super::server::{ConnectionKey, RuntimeServer};
use super::watching::watch_terminal;
use crate::pairings::authorize;

const STARTS_WITH_HELLO: &str = "a runtime connection starts with hello";
const NOT_PAIRED: &str = "this installation is not paired with the cloud runner";
const NOT_ALLOWED: &str = "this pairing does not allow that";
const UNKNOWN_MESSAGE: &str = "this runtime does not know that message";
const ALREADY_FOLLOWING: &str = "this connection already follows the runtime";
const NO_DELEGATIONS: &str = "this connection does not take the delegated requests";
const ALREADY_DELEGATE: &str = "this connection already takes the delegated requests";
const FOREIGN_REQUEST: &str = "that request id is outside this connection's range";
const STOPPED: &str = "the runtime has stopped";

pub(crate) enum Peer {
    Paired {
        install: InstallId,
        permissions: PairingPermissions,
    },
    Unpaired,
}

impl Peer {
    pub(crate) fn allows(&self, permission: PairingPermission) -> bool {
        match self {
            Self::Paired { permissions, .. } => permissions.allows(permission),
            Self::Unpaired => false,
        }
    }

    pub(crate) fn permissions(&self) -> PairingPermissions {
        match self {
            Self::Paired { permissions, .. } => permissions.clone(),
            Self::Unpaired => PairingPermissions::default(),
        }
    }
}

pub(crate) fn refused(code: ErrorCode, message: &str) -> Reply {
    Reply::Refused {
        code,
        message: message.to_owned(),
    }
}

fn refused_message(code: ErrorCode, message: &str) -> Message {
    Message::Refused {
        code,
        message: message.to_owned(),
    }
}

struct Connection {
    server: Arc<RuntimeServer>,
    key: ConnectionKey,
    peer: Peer,
    welcome: Welcome,
    outbox: Outbox,
    answers: Answers,
    following: bool,
    delegations: Option<Delegations>,
}

pub(crate) fn serve_connection(server: &Arc<RuntimeServer>, stream: impl Stream) {
    let (Ok(reading), Ok(kept)) = (stream.try_clone(), stream.try_clone()) else {
        tracing::debug!("a runtime connection could not be shared between threads");
        return;
    };
    let Some(outbox) = Outbox::start(stream) else {
        return;
    };
    let mut input = BufReader::new(reading);
    let key = server.next_key();
    let Some((peer, welcome)) = greet(server, key, &mut input, &outbox) else {
        return;
    };
    let Some(answers) = Answers::start(outbox.clone()) else {
        return;
    };
    if let Peer::Paired { install, .. } = &peer {
        server.admit(install, key, kept);
    }
    let mut connection = Connection {
        server: Arc::clone(server),
        key,
        peer,
        welcome,
        outbox,
        answers,
        following: false,
        delegations: None,
    };
    connection.serve(&mut input);
    if let Peer::Paired { install, .. } = &connection.peer {
        server.leave(install, key);
    }
}

fn greet(
    server: &RuntimeServer,
    key: ConnectionKey,
    input: &mut impl Read,
    outbox: &Outbox,
) -> Option<(Peer, Welcome)> {
    let Ok(Some(Frame::Message(message))) = read_frame(input) else {
        outbox.send(refused_message(ErrorCode::Invalid, STARTS_WITH_HELLO));
        return None;
    };
    let Message::Hello(hello) = *message else {
        outbox.send(refused_message(ErrorCode::Invalid, STARTS_WITH_HELLO));
        return None;
    };
    let Some(peer) = peer_of(server, &hello) else {
        outbox.send(refused_message(ErrorCode::Forbidden, NOT_PAIRED));
        return None;
    };
    match server.handle.hello(&hello) {
        Ok(mut welcome) => {
            welcome.permissions = Some(peer.permissions());
            welcome.first_request = Some(key.first_request());
            outbox.send(Message::Welcome(welcome.clone()));
            Some((peer, welcome))
        }
        Err(Reply::Refused { code, message }) => {
            outbox.send(Message::Refused { code, message });
            None
        }
        Err(Reply::Accepted) => None,
    }
}

fn peer_of(server: &RuntimeServer, hello: &Hello) -> Option<Peer> {
    let Some(pairing) = &hello.pairing else {
        return Some(Peer::Unpaired);
    };
    let permissions = authorize(&server.root, pairing)?;
    Some(Peer::Paired {
        install: pairing.install.clone(),
        permissions,
    })
}

impl Connection {
    fn serve(&mut self, input: &mut impl Read) {
        loop {
            match read_frame(input) {
                Ok(Some(Frame::Message(message))) => self.take(*message),
                Ok(Some(Frame::Terminal(_))) => {
                    self.outbox
                        .send(refused_message(ErrorCode::Unsupported, UNKNOWN_MESSAGE));
                }
                Ok(None) => return,
                Err(error) => {
                    tracing::debug!(%error, "a runtime connection ended");
                    return;
                }
            }
        }
    }

    fn take(&mut self, message: Message) {
        match message {
            Message::Hello(_) => {
                self.outbox.send(Message::Welcome(self.welcome.clone()));
            }
            Message::Subscribe { since } => self.follow(since),
            Message::Command { id, command } => self.command(id, command),
            Message::AskSnapshot { id, scope } => self.snapshot(id, scope),
            Message::Pairing { id, command } => {
                let answer = answer_pairing(&self.server, &self.peer, command);
                self.outbox.send(answer.into_message(id));
            }
            Message::TakeDelegations => self.take_delegations(),
            Message::Welcome(_)
            | Message::Delegated(_)
            | Message::Reply { .. }
            | Message::Feed(_)
            | Message::Snapshot { .. }
            | Message::Paired { .. }
            | Message::Refused { .. }
            | Message::Unknown => {
                self.outbox
                    .send(refused_message(ErrorCode::Unsupported, UNKNOWN_MESSAGE));
            }
        }
    }

    fn reply(&self, id: RequestId, reply: Reply) {
        self.outbox.send(Message::Reply { id, reply });
    }

    fn follow(&mut self, since: Option<Seq>) {
        if !self.peer.allows(PairingPermission::Read) {
            self.outbox
                .send(refused_message(ErrorCode::Forbidden, NOT_ALLOWED));
            return;
        }
        if self.following {
            self.outbox
                .send(refused_message(ErrorCode::Busy, ALREADY_FOLLOWING));
            return;
        }
        self.following = true;
        match since {
            Some(since) => self.outbox.follow(self.server.handle.resume(since)),
            None => self.outbox.follow(self.server.handle.subscribe()),
        }
    }

    fn take_delegations(&mut self) {
        if !self.peer.allows(PairingPermission::Drive) {
            self.outbox
                .send(refused_message(ErrorCode::Forbidden, NOT_ALLOWED));
            return;
        }
        if self.delegations.is_some() {
            self.outbox
                .send(refused_message(ErrorCode::Busy, ALREADY_DELEGATE));
            return;
        }
        let delegations = Delegations::default();
        if let Some(refusal) = take_delegations(&self.server.handle, &self.outbox, &delegations) {
            self.outbox.send(refusal);
            return;
        }
        self.delegations = Some(delegations);
    }

    fn command(&self, id: RequestId, command: Command) {
        if !self.key.owns(id) {
            self.reply(id, refused(ErrorCode::Invalid, FOREIGN_REQUEST));
            return;
        }
        if !self.peer.allows(needed_for_command(&command)) {
            self.reply(id, refused(ErrorCode::Forbidden, NOT_ALLOWED));
            return;
        }
        if let Command::Bus(BusCommand::Respond {
            delegation,
            response,
        }) = command
        {
            let reply = self.delegations.as_ref().map_or_else(
                || refused(ErrorCode::NotFound, NO_DELEGATIONS),
                |delegations| delegations.respond(delegation, response),
            );
            return self.reply(id, reply);
        }
        if let Command::Terminal(TerminalCommand::Watch { terminal }) = command {
            let sinks = (&self.outbox, &self.answers);
            return watch_terminal(&self.server.handle, sinks, id, terminal);
        }
        let pending = self.server.handle.send(id, command);
        self.answers.wait_for(
            pending
                .map(move |reply| Message::Reply {
                    id,
                    reply: reply.unwrap_or_else(|_| refused(ErrorCode::Unavailable, STOPPED)),
                })
                .boxed(),
        );
    }

    fn snapshot(&self, id: RequestId, scope: Scope) {
        if !self.peer.allows(PairingPermission::Read) {
            self.reply(id, refused(ErrorCode::Forbidden, NOT_ALLOWED));
            return;
        }
        let pending = self.server.handle.snapshot(scope);
        self.answers.wait_for(
            pending
                .map(move |snapshot| match snapshot {
                    Ok(snapshot) => Message::Snapshot {
                        id,
                        snapshot: Box::new(snapshot),
                    },
                    Err(_) => Message::Reply {
                        id,
                        reply: refused(ErrorCode::Unavailable, STOPPED),
                    },
                })
                .boxed(),
        );
    }
}
