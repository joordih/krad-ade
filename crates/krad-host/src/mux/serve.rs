#[cfg(unix)]
mod process;
#[cfg(not(unix))]
#[path = "serve/unsupported.rs"]
mod process;
mod sftp_server;
#[cfg(unix)]
mod terminal;
#[cfg(not(unix))]
#[path = "serve/unsupported_terminal.rs"]
mod terminal;

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::bridge::splice_tcp;
use super::channel::Channel;
use super::session::{Closer, Incoming, Session, Side};
use super::wire::OpenRequest;
use super::MuxError;
use crate::layout::APP_VERSION;

pub(crate) use process::Launch;

const REQUEST_THREAD: &str = "krad-ade-mux-request";
const LISTEN_POLL: Duration = Duration::from_millis(100);
const OPEN_WAIT: Duration = Duration::from_secs(30);
const NO_SFTP_SERVER: &str = "the host has no sftp-server";
const CONNECT_REFUSED: &str = "nothing listens on that port of the host";
const LISTEN_FAILED: &str = "the host could not listen on a loopback port";
const NOT_A_LISTENER_REQUEST: &str = "the host only accepts connections for its own listeners";

/// # Errors
pub fn serve(
    input: impl Read + Send + 'static,
    output: impl Write + Send + 'static,
    handshake: Duration,
    closer: Option<Closer>,
) -> Result<(), MuxError> {
    let (session, incoming, peer) =
        Session::start(input, output, Side::Host, APP_VERSION, handshake, closer)?;
    tracing::info!(client = %peer, "serving the multiplexer");
    run(&session, &incoming);
    Ok(())
}

pub(crate) fn run(session: &Session, incoming: &mpsc::Receiver<Incoming>) {
    for request in incoming {
        let session = session.clone();
        let spawned = thread::Builder::new()
            .name(REQUEST_THREAD.to_owned())
            .spawn(move || handle(&session, request));
        if let Err(error) = spawned {
            tracing::warn!(%error, "could not start a request thread");
        }
    }
}

fn handle(session: &Session, Incoming { channel, request }: Incoming) {
    match request {
        OpenRequest::Exec { script } => process::run(&channel, &Launch::Script(script)),
        OpenRequest::Pty { script, size } => terminal::run(&channel, &script, size),
        OpenRequest::Sftp => match sftp_server::locate() {
            Some(server) => process::run(&channel, &Launch::Program(server.program, server.args)),
            None => channel.refuse(NO_SFTP_SERVER),
        },
        OpenRequest::Connect { port } => connect(&channel, port),
        OpenRequest::Listen => listen(session, &channel),
        OpenRequest::Accepted { .. } => channel.refuse(NOT_A_LISTENER_REQUEST),
    }
}

fn connect(channel: &Channel, port: u16) {
    let Ok(tcp) = TcpStream::connect((Ipv4Addr::LOCALHOST, port)) else {
        channel.refuse(CONNECT_REFUSED);
        return;
    };
    if channel.accept(None).is_ok() {
        splice_tcp(channel, &tcp).ok();
    }
}

fn listen(session: &Session, channel: &Channel) {
    let bound = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).and_then(|listener| {
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        Ok((listener, port))
    });
    let Ok((listener, port)) = bound else {
        channel.refuse(LISTEN_FAILED);
        return;
    };
    if channel.accept(Some(port)).is_err() {
        return;
    }
    while !channel.is_closed() {
        match listener.accept() {
            Ok((tcp, _)) => {
                let session = session.clone();
                let listener_id = channel.id();
                let spawned = thread::Builder::new()
                    .name(REQUEST_THREAD.to_owned())
                    .spawn(move || forward_accepted(&session, listener_id, &tcp));
                if let Err(error) = spawned {
                    tracing::warn!(%error, "could not start a forwarded connection");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(LISTEN_POLL);
            }
            Err(error) => {
                tracing::debug!(%error, "the host listener failed");
                break;
            }
        }
    }
    channel.close();
}

fn forward_accepted(session: &Session, listener: u32, tcp: &TcpStream) {
    if tcp.set_nonblocking(false).is_err() {
        return;
    }
    match session.open(OpenRequest::Accepted { listener }, OPEN_WAIT) {
        Ok((channel, _)) => {
            splice_tcp(&channel, tcp).ok();
        }
        Err(error) => tracing::debug!(%error, "the client did not take a forwarded connection"),
    }
}
