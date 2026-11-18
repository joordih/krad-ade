#![cfg(unix)]

use krad_domain::{InstallId, Pairing};
use krad_protocol::{
    read_frame, write_frame, Command, Frame, FrameError, Hello, Message, RequestId, RuntimeCommand,
};
use krad_runner::{bind_runtime_socket, pair_over_ssh, RuntimeServer};
use krad_runtime::{Runtime, RuntimeSetup};
use std::error::Error;
use std::io::{BufReader, BufWriter, Write as _};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

type TestResult = Result<(), Box<dyn Error>>;

const BUILD: &str = "0.0.0-test";
const INSTALL: &str = "0123456789abcdef";
const FLOOD: u64 = 20_000;
const PROGRESS_LIMIT: Duration = Duration::from_secs(20);
const NOT_WELCOMED: &str = "the runtime did not welcome the client";

struct Root(PathBuf);

impl Drop for Root {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn served(name: &str) -> Result<(Root, Pairing), Box<dyn Error>> {
    let root =
        Root(std::env::temp_dir().join(format!("krad-backpressure-{name}-{}", std::process::id())));
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let listener = bind_runtime_socket(&root.0)?;
    let server = RuntimeServer::new(runtime, &root.0);
    thread::spawn(move || server.serve(&listener));
    let install: InstallId = INSTALL.parse()?;
    let token = pair_over_ssh(&root.0, &install, "laptop".parse()?)?;
    Ok((root, Pairing { install, token }))
}

fn send(output: &mut impl std::io::Write, message: Message) -> Result<(), FrameError> {
    write_frame(output, &Frame::Message(Box::new(message)))
}

#[test]
fn a_runtime_keeps_reading_a_client_that_does_not_read_its_answers() -> TestResult {
    let (root, pairing) = served("unread")?;
    let stream = UnixStream::connect(krad_host::runner::runtime_socket(&root.0))?;
    let mut output = BufWriter::new(stream.try_clone()?);
    let mut input = BufReader::new(stream.try_clone()?);
    send(&mut output, Message::Hello(Hello::paired(BUILD, pairing)))?;
    output.flush()?;
    let Some(Frame::Message(welcome)) = read_frame(&mut input)? else {
        return Err(NOT_WELCOMED.into());
    };
    let Message::Welcome(welcome) = *welcome else {
        return Err(NOT_WELCOMED.into());
    };
    let first = welcome.first_request.ok_or(NOT_WELCOMED)?.0;
    send(&mut output, Message::Subscribe { since: None })?;
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        let flooded = (0..FLOOD).all(|offset| {
            let command = Command::Runtime(RuntimeCommand::Ping);
            let id = if offset % 2 == 0 {
                RequestId(first + offset)
            } else {
                RequestId(offset)
            };
            send(&mut output, Message::Command { id, command }).is_ok()
        });
        done.send(flooded && output.flush().is_ok()).ok();
    });
    assert!(finished.recv_timeout(PROGRESS_LIMIT)?);
    stream.set_read_timeout(Some(PROGRESS_LIMIT))?;
    let mut replies = 0;
    while replies < FLOOD {
        let Some(Frame::Message(message)) = read_frame(&mut input)? else {
            continue;
        };
        if matches!(*message, Message::Reply { .. }) {
            replies += 1;
        }
    }
    Ok(())
}
