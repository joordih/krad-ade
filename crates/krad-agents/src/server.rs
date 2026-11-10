mod watch;

pub use watch::HEARTBEAT_INTERVAL;

use crate::error::AgentsError;
use crate::feed::AgentFeed;
use crate::handle::Caller;
use crate::protocol::{
    ApiError, ApiInfo, BusToken, Envelope, ErrorCode, Request, Response, API_VERSION,
};
use futures::channel::{mpsc, oneshot};
use futures::executor::block_on;
use futures::SinkExt;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

const ACCEPT_THREAD_NAME: &str = "krad-agents-accept";
const CONNECTION_THREAD_NAME: &str = "krad-agents-connection";
const MAX_LINE_BYTES: usize = 64 * 1024;
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(10);
const REPLY_POLL_INTERVAL: Duration = Duration::from_millis(20);
const LINE_TOO_LONG: &str = "request line too long";
const INVALID_TOKEN: &str = "invalid token";
const NOT_A_LEAD: &str = "only a lead the user named can call as a lead";
const APP_GONE: &str = "the app is no longer serving agent requests";
const REQUEST_DROPPED: &str = "the app dropped the request";
const CALLER_LEFT: &str = "the caller closed its connection";

#[derive(Clone, Debug)]
pub struct Endpoint {
    pub address: SocketAddr,
    pub token: BusToken,
    pub lead_token: BusToken,
    pub feed: AgentFeed,
}

impl Endpoint {
    fn refusal(&self, envelope: &Envelope) -> Option<ApiError> {
        let expected = match envelope.from {
            Caller::Lead(_) => &self.lead_token,
            Caller::Terminal(_) | Caller::Git(_) | Caller::Client => &self.token,
        };
        if envelope.token == *expected {
            None
        } else if envelope.token == self.token {
            Some(ApiError::new(ErrorCode::Unauthorized, NOT_A_LEAD))
        } else {
            Some(ApiError::new(ErrorCode::Unauthorized, INVALID_TOKEN))
        }
    }
}

#[derive(Debug)]
pub struct Incoming {
    pub from: Caller,
    pub request: Request,
    pub reply: oneshot::Sender<Response>,
}

/// # Errors
pub fn serve(requests: mpsc::Sender<Incoming>) -> Result<Endpoint, AgentsError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let endpoint = Endpoint {
        address: listener.local_addr()?,
        token: crate::token::new_token(),
        lead_token: crate::token::new_token(),
        feed: AgentFeed::new(),
    };
    let served = endpoint.clone();
    thread::Builder::new()
        .name(ACCEPT_THREAD_NAME.to_owned())
        .spawn(move || accept_loop(&listener, &served, &requests))?;
    Ok(endpoint)
}

fn accept_loop(listener: &TcpListener, endpoint: &Endpoint, requests: &mpsc::Sender<Incoming>) {
    for stream in listener.incoming() {
        if requests.is_closed() {
            break;
        }
        let Ok(stream) = stream else { continue };
        let endpoint = endpoint.clone();
        let requests = requests.clone();
        let _spawned = thread::Builder::new()
            .name(CONNECTION_THREAD_NAME.to_owned())
            .spawn(move || serve_connection(&stream, &endpoint, requests));
    }
}

fn serve_connection(stream: &TcpStream, endpoint: &Endpoint, requests: mpsc::Sender<Incoming>) {
    let response = match read_envelope(stream) {
        Ok(envelope) => match endpoint.refusal(&envelope) {
            None => answer(stream, envelope, endpoint, requests),
            Some(refusal) => Some(Response::Error(refusal)),
        },
        Err(error) => Some(Response::Error(error)),
    };
    if let Some(response) = response {
        let _written = write_response(stream, &response);
    }
}

fn answer(
    stream: &TcpStream,
    envelope: Envelope,
    endpoint: &Endpoint,
    requests: mpsc::Sender<Incoming>,
) -> Option<Response> {
    if envelope.api_version != API_VERSION {
        return Some(Response::Error(ApiError::unsupported_version(
            envelope.api_version,
        )));
    }
    match envelope.request {
        Request::Hello => Some(Response::Hello(ApiInfo::current())),
        Request::SubscribeAgents(subscription) => {
            watch::stream_agents(stream, &endpoint.feed, subscription);
            None
        }
        _ => Some(dispatch(stream, envelope, requests)),
    }
}

fn read_envelope(stream: &TcpStream) -> Result<Envelope, ApiError> {
    let bad_request =
        |error: &dyn std::fmt::Display| ApiError::new(ErrorCode::BadRequest, error.to_string());
    stream
        .set_read_timeout(Some(REQUEST_READ_TIMEOUT))
        .map_err(|error| bad_request(&error))?;
    let mut line = String::new();
    BufReader::new(stream.take(MAX_LINE_BYTES as u64 + 1))
        .read_line(&mut line)
        .map_err(|error| bad_request(&error))?;
    if line.len() > MAX_LINE_BYTES {
        return Err(ApiError::new(ErrorCode::BadRequest, LINE_TOO_LONG));
    }
    serde_json::from_str(&line).map_err(|error| bad_request(&error))
}

fn dispatch(
    stream: &TcpStream,
    envelope: Envelope,
    mut requests: mpsc::Sender<Incoming>,
) -> Response {
    let deadline = envelope.request.deadline();
    let (reply, reply_rx) = oneshot::channel();
    let incoming = Incoming {
        from: envelope.from,
        request: envelope.request,
        reply,
    };
    if block_on(requests.send(incoming)).is_err() {
        return Response::error(ErrorCode::Unavailable, APP_GONE);
    }
    await_reply(stream, reply_rx, deadline)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Peer {
    Waiting,
    Left,
}

fn watch_peer(stream: &TcpStream) -> Peer {
    let mut probe = [0_u8; 1];
    match stream.peek(&mut probe) {
        Ok(0) => Peer::Left,
        Ok(_) => {
            thread::sleep(REPLY_POLL_INTERVAL);
            Peer::Waiting
        }
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
            ) =>
        {
            Peer::Waiting
        }
        Err(_) => Peer::Left,
    }
}

fn await_reply(
    stream: &TcpStream,
    mut reply_rx: oneshot::Receiver<Response>,
    deadline: Duration,
) -> Response {
    let watched = stream.set_read_timeout(Some(REPLY_POLL_INTERVAL)).is_ok();
    let started = Instant::now();
    while started.elapsed() < deadline {
        match reply_rx.try_recv() {
            Ok(Some(response)) => return response,
            Ok(None) if !watched => thread::sleep(REPLY_POLL_INTERVAL),
            Ok(None) => {
                if watch_peer(stream) == Peer::Left {
                    return Response::error(ErrorCode::Unavailable, CALLER_LEFT);
                }
            }
            Err(oneshot::Canceled) => {
                return Response::error(ErrorCode::Unavailable, REQUEST_DROPPED)
            }
        }
    }
    Response::TimedOut
}

fn write_response(mut stream: &TcpStream, response: &Response) -> Result<(), AgentsError> {
    let mut line = serde_json::to_string(response)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    Ok(())
}
