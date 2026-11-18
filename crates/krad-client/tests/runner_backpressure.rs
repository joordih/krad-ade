#![cfg(unix)]

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_client::{Runner, RuntimeClient};
use krad_protocol::{
    read_frame, write_frame, Command, Event, EventKind, Feed, Frame, Hello, Message, RequestId,
    RuntimeCommand, RuntimeRun, Seq,
};
use std::error::Error;
use std::io::{BufReader, BufWriter, Write as _};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

type TestResult = Result<(), Box<dyn Error>>;

const BUILD: &str = "0.0.0-test";
const FLOOD: u64 = 20_000;
const PROGRESS_LIMIT: Duration = Duration::from_secs(10);
const NOT_GREETED: &str = "the client did not say hello";

fn welcomed() -> Result<(Runner, UnixStream), Box<dyn Error>> {
    let (ours, theirs) = UnixStream::pair()?;
    let greeting = thread::spawn(move || -> Result<UnixStream, String> {
        let mut input = BufReader::new(theirs.try_clone().map_err(|error| error.to_string())?);
        let Ok(Some(Frame::Message(hello))) = read_frame(&mut input) else {
            return Err(NOT_GREETED.to_owned());
        };
        let Message::Hello(hello) = *hello else {
            return Err(NOT_GREETED.to_owned());
        };
        let welcome = hello
            .welcome(BUILD, RuntimeRun(1))
            .map_err(|_| NOT_GREETED.to_owned())?;
        let mut output = BufWriter::new(theirs.try_clone().map_err(|error| error.to_string())?);
        write_frame(
            &mut output,
            &Frame::Message(Box::new(Message::Welcome(welcome))),
        )
        .map_err(|error| error.to_string())?;
        output.flush().map_err(|error| error.to_string())?;
        Ok(theirs)
    });
    let client = Runner::connect(ours.try_clone()?, ours, Hello::new(BUILD))?;
    let service = greeting.join().map_err(|_| NOT_GREETED)??;
    Ok((client, service))
}

fn pong(seq: u64) -> Frame {
    Frame::Message(Box::new(Message::Feed(Feed::Event(Event {
        seq: Seq(seq),
        kind: EventKind::Pong {
            request: RequestId(seq),
        },
    }))))
}

#[test]
fn a_client_keeps_sending_while_its_runner_reads_nothing() -> TestResult {
    let (client, service) = welcomed()?;
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        for _ in 0..FLOOD {
            client.post(Command::Runtime(RuntimeCommand::Ping));
        }
        done.send(()).ok();
    });
    finished.recv_timeout(PROGRESS_LIMIT)?;
    drop(service);
    Ok(())
}

#[test]
fn a_client_keeps_reading_its_feed_while_nobody_takes_it() -> TestResult {
    let (client, service) = welcomed()?;
    let feeds = client.subscribe(None);
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        let mut output = BufWriter::new(service);
        let written = (1..=FLOOD).all(|seq| write_frame(&mut output, &pong(seq)).is_ok());
        done.send(written && output.flush().is_ok()).ok();
    });
    assert!(finished.recv_timeout(PROGRESS_LIMIT)?);
    let read: Vec<Feed> = block_on(feeds.take(usize::try_from(FLOOD)?).collect());
    assert_eq!(read.len(), usize::try_from(FLOOD)?);
    Ok(())
}
