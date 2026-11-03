use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::thread;
use std::time::Duration;

use super::bridge::copy_until_end;
use super::broker::{askpass_header, exec_header, ANSWER_OK};
use super::session::{Session, Side};
use super::wire::{OpenRequest, Stream};
use super::{MuxError, ENV_ENDPOINT, ENV_TOKEN};
use crate::layout::APP_VERSION;

const EXIT_FAILED: i32 = 125;
const EXIT_NO_ANSWER: i32 = 1;
const EXIT_OK: i32 = 0;
const HANDSHAKE_WAIT: Duration = Duration::from_secs(10);
const OPEN_WAIT: Duration = Duration::from_secs(60);
const INPUT_THREAD: &str = "krad-ade-mux-shim-input";
const SPACE: char = ' ';

fn broker() -> Result<(TcpStream, String), MuxError> {
    let endpoint = std::env::var(ENV_ENDPOINT).map_err(|_| MuxError::Closed)?;
    let token = std::env::var(ENV_TOKEN).map_err(|_| MuxError::Closed)?;
    Ok((TcpStream::connect(endpoint)?, token))
}

#[must_use]
pub fn run_exec(id: &str) -> i32 {
    match exec(id) {
        Ok(code) => code,
        Err(error) => {
            io::stderr()
                .write_all(format!("krad-ade-agents: {error}\n").as_bytes())
                .ok();
            EXIT_FAILED
        }
    }
}

fn exec(id: &str) -> Result<i32, MuxError> {
    let (mut stream, token) = broker()?;
    stream.write_all(exec_header(&token).as_bytes())?;
    let closer = stream.try_clone()?;
    let (session, _, _) = Session::start(
        BufReader::new(stream.try_clone()?),
        stream,
        Side::Client,
        APP_VERSION,
        HANDSHAKE_WAIT,
        Some(Box::new(move || {
            closer.shutdown(Shutdown::Both).ok();
        })),
    )?;
    let (channel, _) = session.open(
        OpenRequest::Exec {
            script: id.to_owned(),
        },
        OPEN_WAIT,
    )?;
    let mut input = channel.writer(Stream::Main);
    thread::Builder::new()
        .name(INPUT_THREAD.to_owned())
        .spawn(move || {
            copy_until_end(io::stdin(), &mut input).ok();
            input.finish();
        })?;
    let errors = channel.reader(Stream::Errors);
    let copying_errors = thread::Builder::new()
        .name(INPUT_THREAD.to_owned())
        .spawn(move || {
            copy_until_end(errors, io::stderr()).ok();
        })?;
    copy_until_end(channel.reader(Stream::Main), io::stdout())?;
    copying_errors.join().ok();
    let code = channel.wait_exit(None)?;
    session.close();
    Ok(code.unwrap_or(EXIT_FAILED))
}

#[must_use]
pub fn run_askpass(prompt: &str) -> i32 {
    match askpass(prompt) {
        Ok(Some(answer)) => {
            let mut out = io::stdout();
            if out
                .write_all(answer.as_bytes())
                .and_then(|()| out.write_all(b"\n"))
                .and_then(|()| out.flush())
                .is_ok()
            {
                EXIT_OK
            } else {
                EXIT_FAILED
            }
        }
        Ok(None) => EXIT_NO_ANSWER,
        Err(_) => EXIT_FAILED,
    }
}

fn askpass(prompt: &str) -> Result<Option<String>, MuxError> {
    let (mut stream, token) = broker()?;
    stream.write_all(askpass_header(&token, prompt).as_bytes())?;
    stream.write_all(prompt.as_bytes())?;
    stream.flush()?;
    let mut reader = BufReader::new(stream);
    let mut header = String::new();
    reader.read_line(&mut header)?;
    let mut words = header.trim_end().split(SPACE);
    if words.next() != Some(ANSWER_OK) {
        return Ok(None);
    }
    let len = words
        .next()
        .and_then(|len| len.parse::<usize>().ok())
        .ok_or(MuxError::Protocol)?;
    let mut answer = vec![0; len];
    reader.read_exact(&mut answer)?;
    String::from_utf8(answer)
        .map(Some)
        .map_err(|_| MuxError::Protocol)
}
