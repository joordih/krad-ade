use crate::error::AgentsError;
use crate::handle::Caller;
use crate::protocol::{AgentSubscription, BusToken, Envelope, Request, Response, API_VERSION};
use crate::server::HEARTBEAT_INTERVAL;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::TcpStream;
use std::ops::ControlFlow;
use std::time::Duration;

const READ_MARGIN: Duration = Duration::from_secs(2);
const MISSED_HEARTBEATS: u32 = 3;

fn send(
    endpoint: &str,
    token: &BusToken,
    from: &Caller,
    request: Request,
    read_timeout: Duration,
) -> Result<TcpStream, AgentsError> {
    let mut stream = TcpStream::connect(endpoint)?;
    stream.set_read_timeout(Some(read_timeout))?;
    let envelope = Envelope {
        api_version: API_VERSION,
        token: token.clone(),
        from: from.clone(),
        request,
    };
    let mut line = serde_json::to_string(&envelope)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    Ok(stream)
}

/// # Errors
pub fn call(
    endpoint: &str,
    token: &BusToken,
    from: &Caller,
    request: Request,
) -> Result<Response, AgentsError> {
    let read_timeout = request.deadline() + READ_MARGIN;
    let stream = send(endpoint, token, from, request, read_timeout)?;
    let mut answer = String::new();
    if BufReader::new(&stream).read_line(&mut answer)? == 0 {
        return Err(AgentsError::ConnectionClosed);
    }
    Ok(serde_json::from_str(&answer)?)
}

/// # Errors
pub fn watch(
    endpoint: &str,
    token: &BusToken,
    from: &Caller,
    subscription: AgentSubscription,
    mut each: impl FnMut(Response) -> ControlFlow<()>,
) -> Result<(), AgentsError> {
    let request = Request::SubscribeAgents(subscription);
    let stream = send(
        endpoint,
        token,
        from,
        request,
        HEARTBEAT_INTERVAL * MISSED_HEARTBEATS,
    )?;
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return Err(AgentsError::ConnectionClosed),
            Ok(_) => {}
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return Err(AgentsError::NoHeartbeat)
            }
            Err(error) => return Err(error.into()),
        }
        if each(serde_json::from_str(&line)?).is_break() {
            return Ok(());
        }
    }
}
