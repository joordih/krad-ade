#[cfg(test)]
mod tests;

use krad_domain::TerminalId;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

use crate::command::{Command, ErrorCode, Reply};
use crate::event::{Feed, Scope, Snapshot};
use crate::hello::{Hello, Welcome};
use crate::ids::{RequestId, Seq};
use crate::pairings::{PairingAnswer, PairingCommand};

pub(crate) const LENGTH_BYTES: usize = 4;
pub(crate) const TERMINAL_ID_BYTES: usize = 8;
pub(crate) const MAX_FRAME_BYTES: u32 = 8 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub(crate) enum FrameKind {
    Message = 1,
    Terminal = 2,
}

impl TryFrom<u8> for FrameKind {
    type Error = FrameError;

    fn try_from(byte: u8) -> Result<Self, FrameError> {
        [Self::Message, Self::Terminal]
            .into_iter()
            .find(|kind| *kind as u8 == byte)
            .ok_or(FrameError::UnknownKind(byte))
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Hello(Hello),
    Welcome(Welcome),
    Subscribe {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<Seq>,
    },
    Command {
        id: RequestId,
        command: Command,
    },
    Reply {
        id: RequestId,
        reply: Reply,
    },
    Feed(Feed),
    AskSnapshot {
        id: RequestId,
        scope: Scope,
    },
    Snapshot {
        id: RequestId,
        snapshot: Box<Snapshot>,
    },
    Pairing {
        id: RequestId,
        command: PairingCommand,
    },
    Paired {
        id: RequestId,
        answer: PairingAnswer,
    },
    Refused {
        code: ErrorCode,
        message: String,
    },
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TerminalBytes {
    pub terminal: TerminalId,
    pub bytes: Vec<u8>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Frame {
    Message(Box<Message>),
    Terminal(TerminalBytes),
}

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("could not read or write a frame")]
    Io(#[from] io::Error),
    #[error("the stream ended inside a frame")]
    Truncated,
    #[error("a frame is over the {MAX_FRAME_BYTES} byte limit")]
    TooLarge,
    #[error("a frame has no kind")]
    Empty,
    #[error("unknown frame kind {0}")]
    UnknownKind(u8),
    #[error("a terminal frame is shorter than its terminal id")]
    MissingTerminal,
    #[error("a message frame does not hold a message")]
    Json(#[from] serde_json::Error),
}

/// # Errors
///
/// Fails when the stream fails or ends inside a frame, or when the frame is too large, of an
/// unknown kind or not a valid message.
pub fn read_frame(input: &mut impl Read) -> Result<Option<Frame>, FrameError> {
    let mut header = [0; LENGTH_BYTES];
    if !read_header(input, &mut header)? {
        return Ok(None);
    }
    let len = u32::from_be_bytes(header);
    if len > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }
    let mut body = vec![0; usize::try_from(len).map_err(|_| FrameError::TooLarge)?];
    input.read_exact(&mut body).map_err(ended_inside)?;
    let (&kind, payload) = body.split_first().ok_or(FrameError::Empty)?;
    match FrameKind::try_from(kind)? {
        FrameKind::Message => Ok(Some(Frame::Message(Box::new(serde_json::from_slice(
            payload,
        )?)))),
        FrameKind::Terminal => read_terminal(payload).map(|bytes| Some(Frame::Terminal(bytes))),
    }
}

/// # Errors
///
/// Fails when the frame is too large, its message cannot be serialized or the stream fails.
pub fn write_frame(output: &mut impl Write, frame: &Frame) -> Result<(), FrameError> {
    let mut body = Vec::new();
    match frame {
        Frame::Message(message) => {
            body.push(FrameKind::Message as u8);
            serde_json::to_writer(&mut body, message)?;
        }
        Frame::Terminal(terminal) => {
            body.push(FrameKind::Terminal as u8);
            body.extend_from_slice(&terminal.terminal.as_u64().to_be_bytes());
            body.extend_from_slice(&terminal.bytes);
        }
    }
    let len = u32::try_from(body.len())
        .ok()
        .filter(|len| *len <= MAX_FRAME_BYTES)
        .ok_or(FrameError::TooLarge)?;
    output.write_all(&len.to_be_bytes())?;
    output.write_all(&body)?;
    Ok(())
}

fn read_header(input: &mut impl Read, header: &mut [u8; LENGTH_BYTES]) -> Result<bool, FrameError> {
    let mut filled = 0;
    while filled < LENGTH_BYTES {
        match input.read(&mut header[filled..]) {
            Ok(0) if filled == 0 => return Ok(false),
            Ok(0) => return Err(FrameError::Truncated),
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(true)
}

fn read_terminal(payload: &[u8]) -> Result<TerminalBytes, FrameError> {
    let (id, bytes) = payload
        .split_first_chunk::<TERMINAL_ID_BYTES>()
        .ok_or(FrameError::MissingTerminal)?;
    Ok(TerminalBytes {
        terminal: TerminalId(u64::from_be_bytes(*id)),
        bytes: bytes.to_vec(),
    })
}

fn ended_inside(error: io::Error) -> FrameError {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        FrameError::Truncated
    } else {
        FrameError::Io(error)
    }
}
