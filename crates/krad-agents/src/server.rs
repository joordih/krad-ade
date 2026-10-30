use crate::error::AgentsError;
use crate::handle::Caller;
use crate::protocol::{BusToken, Envelope, Request, Response};
use futures::channel::{mpsc, oneshot};
use futures::executor::block_on;
use futures::SinkExt;
use std::io::{BufRead, BufReader, Read, Write};
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

#[derive(Clone, Debug)]
pub struct Endpoint {
    pub address: SocketAddr,
    pub token: BusToken,
    pub lead_token: BusToken,
}

impl Endpoint {
    fn refusal(&self, envelope: &Envelope) -> Option<&'static str> {
        let expected = match envelope.from {
            Caller::Lead(_) => &self.lead_token,
            Caller::Terminal(_) => &self.token,
        };
        if envelope.token == *expected {
            None
        } else if envelope.token == self.token {
            Some(NOT_A_LEAD)
        } else {
            Some(INVALID_TOKEN)
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
            None => dispatch(envelope, requests),
            Some(refusal) => Response::Error(refusal.to_owned()),
        },
        Err(message) => Response::Error(message),
    };
    let _written = write_response(stream, &response);
}

fn read_envelope(stream: &TcpStream) -> Result<Envelope, String> {
    stream
        .set_read_timeout(Some(REQUEST_READ_TIMEOUT))
        .map_err(|error| error.to_string())?;
    let mut line = String::new();
    BufReader::new(stream.take(MAX_LINE_BYTES as u64 + 1))
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    if line.len() > MAX_LINE_BYTES {
        return Err(LINE_TOO_LONG.to_owned());
    }
    serde_json::from_str(&line).map_err(|error| error.to_string())
}

fn dispatch(envelope: Envelope, mut requests: mpsc::Sender<Incoming>) -> Response {
    let deadline = envelope.request.deadline();
    let (reply, reply_rx) = oneshot::channel();
    let incoming = Incoming {
        from: envelope.from,
        request: envelope.request,
        reply,
    };
    if block_on(requests.send(incoming)).is_err() {
        return Response::Error(APP_GONE.to_owned());
    }
    await_reply(reply_rx, deadline)
}

fn await_reply(mut reply_rx: oneshot::Receiver<Response>, deadline: Duration) -> Response {
    let started = Instant::now();
    while started.elapsed() < deadline {
        match reply_rx.try_recv() {
            Ok(Some(response)) => return response,
            Ok(None) => thread::sleep(REPLY_POLL_INTERVAL),
            Err(oneshot::Canceled) => return Response::Error(REQUEST_DROPPED.to_owned()),
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
