use krad_domain::{CloudProcessId, JournalOffset, Pairing};
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;

use super::header::{JournalHeader, PastEndNotice};
use super::status::CloudProcessStatus;
use super::wire::{CloudLaunch, Envelope, Frame, Reply, Request};
use crate::mux::TerminalSize;

const INPUT_THREAD: &str = "krad-cloud-attach-input";
const DAEMON_HUNG_UP: &str = "the cloud runner closed the connection before the process ended";
const UNEXPECTED_REPLY: &str = "the cloud runner sent an unexpected reply";
const OFFSET_PAST_END: &str = "the cloud runner's journal is shorter than that offset";
const CHUNK_BYTES: usize = 32 * 1024;

fn send(socket: &Path, pairing: &Pairing, request: Request) -> io::Result<(UnixStream, Reply)> {
    let mut stream = UnixStream::connect(socket)?;
    Envelope {
        pairing: pairing.clone(),
        request,
    }
    .write(&mut stream)?;
    match Reply::read(&mut stream)? {
        Reply::Refused(reason) => Err(io::Error::other(reason)),
        reply => Ok((stream, reply)),
    }
}

/// # Errors
pub fn attach(
    socket: &Path,
    pairing: &Pairing,
    id: CloudProcessId,
    from: JournalOffset,
    launch: Option<CloudLaunch>,
    input: impl Read + Send + 'static,
    output: &mut impl Write,
) -> io::Result<Option<i32>> {
    let (mut stream, reply) = send(socket, pairing, Request::Open { id, from, launch })?;
    let end = match reply {
        Reply::Accepted { end } => end,
        Reply::PastEnd { end } => {
            writeln!(output, "{}", PastEndNotice { end })?;
            output.flush()?;
            return Err(io::Error::new(io::ErrorKind::InvalidInput, OFFSET_PAST_END));
        }
        _ => return Err(io::Error::other(UNEXPECTED_REPLY)),
    };
    write_header(output, JournalHeader { from, end })?;
    forward_input(input, stream.try_clone()?)?;
    loop {
        match Frame::read(&mut stream) {
            Ok(Frame::Data(bytes)) => {
                output.write_all(&bytes)?;
                output.flush()?;
            }
            Ok(Frame::Exit(code)) => return Ok(code),
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    DAEMON_HUNG_UP,
                ));
            }
            Err(error) => return Err(error),
        }
    }
}

fn write_header(output: &mut impl Write, header: JournalHeader) -> io::Result<()> {
    writeln!(output, "{header}")?;
    output.flush()
}

/// # Errors
pub fn stop(socket: &Path, pairing: &Pairing, id: CloudProcessId) -> io::Result<()> {
    expect_done(&send(socket, pairing, Request::Stop { id })?.1)
}

/// # Errors
pub fn resize(
    socket: &Path,
    pairing: &Pairing,
    id: CloudProcessId,
    size: TerminalSize,
) -> io::Result<()> {
    expect_done(&send(socket, pairing, Request::Resize { id, size })?.1)
}

/// # Errors
pub fn list(socket: &Path, pairing: &Pairing) -> io::Result<Vec<CloudProcessStatus>> {
    match send(socket, pairing, Request::List)?.1 {
        Reply::Listed(statuses) => Ok(statuses),
        _ => Err(io::Error::other(UNEXPECTED_REPLY)),
    }
}

fn expect_done(reply: &Reply) -> io::Result<()> {
    match reply {
        Reply::Done => Ok(()),
        _ => Err(io::Error::other(UNEXPECTED_REPLY)),
    }
}

fn forward_input(mut input: impl Read + Send + 'static, mut stream: UnixStream) -> io::Result<()> {
    thread::Builder::new()
        .name(INPUT_THREAD.to_owned())
        .spawn(move || {
            let mut chunk = vec![0; CHUNK_BYTES];
            while let Ok(read) = input.read(&mut chunk) {
                if read == 0 || stream.write_all(&chunk[..read]).is_err() {
                    break;
                }
            }
            stream.shutdown(Shutdown::Write).ok();
        })?;
    Ok(())
}
