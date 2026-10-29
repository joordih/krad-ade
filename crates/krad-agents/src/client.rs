use crate::error::AgentsError;
use crate::handle::Caller;
use crate::protocol::{BusToken, Envelope, Request, Response};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

const READ_MARGIN: Duration = Duration::from_secs(2);

/// # Errors
pub fn call(
    endpoint: &str,
    token: &BusToken,
    from: &Caller,
    request: Request,
) -> Result<Response, AgentsError> {
    let mut stream = TcpStream::connect(endpoint)?;
    stream.set_read_timeout(Some(request.deadline() + READ_MARGIN))?;
    let envelope = Envelope {
        token: token.clone(),
        from: from.clone(),
        request,
    };
    let mut line = serde_json::to_string(&envelope)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;

    let mut answer = String::new();
    if BufReader::new(&stream).read_line(&mut answer)? == 0 {
        return Err(AgentsError::ConnectionClosed);
    }
    Ok(serde_json::from_str(&answer)?)
}
