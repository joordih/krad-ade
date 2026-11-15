use krad_domain::HostedOrigin;
use krad_host::HostLink;
use krad_protocol::{
    read_frame, write_frame, CloudChatStart, CloudCommand, Command, Frame, FrameError, Hello,
    HostedSession, HostedWorkspace, Message, Reply, RequestId,
};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use super::carry::{carry_chats, CarriedChat};
use super::workspace::{placement_on, WorkspaceError, WorkspaceRequest};
use crate::runners::{open_runner_line, RunnerLine, RunnerLineError};

const FIRST_REQUEST: RequestId = RequestId(1);
const NO_WELCOME: &str = "the runner's runtime did not welcome this computer";
const HUNG_UP: &str = "the runner's runtime closed the connection before it answered";
const TOO_SLOW: &str = "the runner's runtime did not answer in time";
const NO_READER: &str = "the hand-over could not start its reader";
const READER_THREAD: &str = "krad-runtime-hand-over";
const HAND_OVER_LIMIT: Duration = Duration::from_secs(60);
const READ_AHEAD: usize = 16;

#[derive(Debug)]
pub(crate) struct HandOver {
    pub(crate) link: HostLink,
    pub(crate) cli: PathBuf,
    pub(crate) hello: Hello,
    pub(crate) name: String,
    pub(crate) request: WorkspaceRequest,
    pub(crate) carried: Vec<CarriedChat>,
    pub(crate) starts: Vec<CloudChatStart>,
    pub(crate) origin: Option<HostedOrigin>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum HandOverError {
    #[error("{0}")]
    Workspace(#[from] WorkspaceError),
    #[error("{0}")]
    Line(#[from] RunnerLineError),
    #[error("the runner's runtime could not be spoken to: {0}")]
    Frame(#[from] FrameError),
    #[error("{0}")]
    Unanswered(&'static str),
    #[error("the runner's runtime refused the cloud session: {0}")]
    Refused(String),
    #[error("the runner has no runtime that welcomes this computer: {0}")]
    NoRuntime(String),
}

fn unwelcomed(error: HandOverError) -> HandOverError {
    match error {
        HandOverError::NoRuntime(_) => error,
        error => HandOverError::NoRuntime(error.to_string()),
    }
}

fn send(output: &mut impl Write, message: Message) -> Result<(), HandOverError> {
    write_frame(output, &Frame::Message(Box::new(message)))?;
    output.flush().map_err(FrameError::from)?;
    Ok(())
}

type FrameRead = Result<Option<Frame>, FrameError>;

fn read_ahead(input: impl Read + Send + 'static) -> Result<Receiver<FrameRead>, HandOverError> {
    let (frames, read) = sync_channel(READ_AHEAD);
    let mut input = BufReader::new(input);
    thread::Builder::new()
        .name(READER_THREAD.to_owned())
        .spawn(move || loop {
            let frame = read_frame(&mut input);
            let last = !matches!(frame, Ok(Some(_)));
            if frames.send(frame).is_err() || last {
                return;
            }
        })
        .map_err(|_| HandOverError::Unanswered(NO_READER))?;
    Ok(read)
}

fn next_message(frames: &Receiver<FrameRead>, deadline: Instant) -> Result<Message, HandOverError> {
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let frame = match frames.recv_timeout(left) {
            Ok(frame) => frame?,
            Err(RecvTimeoutError::Timeout) => return Err(HandOverError::Unanswered(TOO_SLOW)),
            Err(RecvTimeoutError::Disconnected) => return Err(HandOverError::Unanswered(HUNG_UP)),
        };
        match frame {
            Some(Frame::Message(message)) => return Ok(*message),
            Some(Frame::Terminal(_)) => {}
            None => return Err(HandOverError::Unanswered(HUNG_UP)),
        }
    }
}

fn exchange(line: RunnerLine, command: Command, limit: Duration) -> Result<Reply, HandOverError> {
    let RunnerLine {
        input,
        output,
        hello,
        stop,
    } = line;
    let answered = talk(input, output, hello, command, Instant::now() + limit);
    stop.stop();
    answered
}

fn talk(
    input: impl Read + Send + 'static,
    output: impl Write,
    hello: Hello,
    command: Command,
    deadline: Instant,
) -> Result<Reply, HandOverError> {
    let mut output = BufWriter::new(output);
    let frames = read_ahead(input)?;
    let input = &frames;
    send(&mut output, Message::Hello(hello)).map_err(unwelcomed)?;
    let welcome = match next_message(input, deadline).map_err(unwelcomed)? {
        Message::Welcome(welcome) => welcome,
        Message::Refused { message, .. } => return Err(HandOverError::NoRuntime(message)),
        _ => return Err(HandOverError::NoRuntime(NO_WELCOME.to_owned())),
    };
    let id = welcome.first_request.unwrap_or(FIRST_REQUEST);
    send(&mut output, Message::Command { id, command })?;
    loop {
        match next_message(input, deadline)? {
            Message::Reply {
                id: answered,
                reply,
            } if answered == id => return Ok(reply),
            Message::Refused { message, .. } => return Err(HandOverError::Refused(message)),
            _ => {}
        }
    }
}

#[tracing::instrument(skip_all, fields(runner = %over.request.runner))]
pub(crate) fn hand_over(over: HandOver) -> Result<(), HandOverError> {
    let placement = placement_on(&over.link, &over.request)?;
    let mut starts: Vec<_> = carry_chats(&over.link, &placement.worktree, over.carried)
        .into_iter()
        .map(|moved| moved.start)
        .collect();
    starts.extend(over.starts);
    let WorkspaceRequest {
        remote_url,
        upstream_branch,
        session_slug,
        patch,
        ..
    } = over.request;
    let session = Box::new(HostedSession {
        name: over.name,
        workspace: HostedWorkspace {
            remote_url,
            upstream_branch,
            session_slug,
            patch,
        },
        starts,
        origin: over.origin,
    });
    let line = open_runner_line(&over.link, &over.cli, over.hello)
        .map_err(|error| unwelcomed(error.into()))?;
    let command = Command::Cloud(CloudCommand::Host { session });
    if let Reply::Refused { message, .. } = exchange(line, command, HAND_OVER_LIMIT)? {
        return Err(HandOverError::Refused(message));
    }
    tracing::info!("cloud session handed to the runner's runtime");
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use krad_protocol::RuntimeCommand;
    use std::os::unix::net::UnixStream;

    const BUILD: &str = "0.0.0";
    const SHORT: Duration = Duration::from_millis(50);

    #[test]
    fn a_runner_that_never_answers_the_hand_over_is_given_up_on() -> std::io::Result<()> {
        let (ours, theirs) = UnixStream::pair()?;
        let command = Command::Runtime(RuntimeCommand::Ping);
        let deadline = Instant::now() + SHORT;
        let talked = talk(
            ours.try_clone()?,
            ours,
            Hello::new(BUILD),
            command,
            deadline,
        );
        assert!(
            matches!(talked, Err(HandOverError::NoRuntime(ref reason)) if reason == TOO_SLOW),
            "{talked:?}"
        );
        drop(theirs);
        Ok(())
    }

    #[test]
    fn a_runner_that_hangs_up_before_its_welcome_has_no_runtime_to_hand_over_to(
    ) -> std::io::Result<()> {
        let (ours, theirs) = UnixStream::pair()?;
        drop(theirs);
        let talked = talk(
            ours.try_clone()?,
            ours,
            Hello::new(BUILD),
            Command::Runtime(RuntimeCommand::Ping),
            Instant::now() + SHORT,
        );
        assert!(
            matches!(talked, Err(HandOverError::NoRuntime(_))),
            "{talked:?}"
        );
        Ok(())
    }
}
