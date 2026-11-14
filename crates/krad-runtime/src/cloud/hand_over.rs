use krad_host::HostLink;
use krad_protocol::{
    read_frame, write_frame, CloudChatStart, CloudCommand, Command, Frame, FrameError, Hello,
    HostedSession, HostedWorkspace, Message, Reply, RequestId,
};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;

use super::carry::{carry_chats, CarriedChat};
use super::workspace::{placement_on, WorkspaceError, WorkspaceRequest};
use crate::runners::{open_runner_line, RunnerLine, RunnerLineError};

const FIRST_REQUEST: RequestId = RequestId(1);
const NO_WELCOME: &str = "the runner's runtime did not welcome this computer";
const HUNG_UP: &str = "the runner's runtime closed the connection before it answered";

#[derive(Debug)]
pub(crate) struct HandOver {
    pub(crate) link: HostLink,
    pub(crate) cli: PathBuf,
    pub(crate) hello: Hello,
    pub(crate) name: String,
    pub(crate) request: WorkspaceRequest,
    pub(crate) carried: Vec<CarriedChat>,
    pub(crate) starts: Vec<CloudChatStart>,
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
}

fn send(output: &mut impl Write, message: Message) -> Result<(), HandOverError> {
    write_frame(output, &Frame::Message(Box::new(message)))?;
    output.flush().map_err(FrameError::from)?;
    Ok(())
}

fn next_message(input: &mut impl Read) -> Result<Message, HandOverError> {
    loop {
        match read_frame(input)? {
            Some(Frame::Message(message)) => return Ok(*message),
            Some(Frame::Terminal(_)) => {}
            None => return Err(HandOverError::Unanswered(HUNG_UP)),
        }
    }
}

fn exchange(line: RunnerLine, command: Command) -> Result<Reply, HandOverError> {
    let RunnerLine {
        input,
        output,
        hello,
    } = line;
    let mut output = BufWriter::new(output);
    let mut input = BufReader::new(input);
    send(&mut output, Message::Hello(hello))?;
    let welcome = match next_message(&mut input)? {
        Message::Welcome(welcome) => welcome,
        Message::Refused { message, .. } => return Err(HandOverError::Refused(message)),
        _ => return Err(HandOverError::Unanswered(NO_WELCOME)),
    };
    let id = welcome.first_request.unwrap_or(FIRST_REQUEST);
    send(&mut output, Message::Command { id, command })?;
    loop {
        match next_message(&mut input)? {
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
    let mut starts = carry_chats(&over.link, &placement.worktree, over.carried);
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
    });
    let line = open_runner_line(&over.link, &over.cli, over.hello)?;
    let command = Command::Cloud(CloudCommand::Host { session });
    if let Reply::Refused { message, .. } = exchange(line, command)? {
        return Err(HandOverError::Refused(message));
    }
    tracing::info!("cloud session handed to the runner's runtime");
    Ok(())
}
