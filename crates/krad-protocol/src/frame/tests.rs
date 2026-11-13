use super::*;
use crate::command::{ErrorCode, RuntimeCommand};
use crate::event::{Event, EventKind, Scope, Snapshot};
use crate::ids::Seq;
use std::io::Cursor;

type ExpectedError = fn(&FrameError) -> bool;

fn encoded(frames: &[Frame]) -> Result<Vec<u8>, FrameError> {
    let mut out = Vec::new();
    for frame in frames {
        write_frame(&mut out, frame)?;
    }
    Ok(out)
}

fn framed(body: &[u8]) -> Vec<u8> {
    let mut out = u32::try_from(body.len())
        .unwrap_or(u32::MAX)
        .to_be_bytes()
        .to_vec();
    out.extend_from_slice(body);
    out
}

#[test]
fn frames_round_trip_in_order_and_the_stream_ends_cleanly() -> Result<(), FrameError> {
    let frames = vec![
        Frame::Message(Box::new(Message::Hello {
            version: ProtocolVersion::CURRENT,
        })),
        Frame::Message(Box::new(Message::Command {
            id: RequestId(1),
            command: Command::Runtime(RuntimeCommand::Ping),
        })),
        Frame::Message(Box::new(Message::Reply {
            id: RequestId(1),
            reply: Reply::Refused {
                code: ErrorCode::Unsupported,
                message: "unknown".to_owned(),
            },
        })),
        Frame::Message(Box::new(Message::Feed(Feed::Snapshot(Snapshot {
            scope: Scope::Runtime,
            seq: Seq(1),
            bus: crate::bus::BusSnapshot::default(),
            attention: crate::attention::AttentionSnapshot::default(),
            health: crate::HealthSnapshot::default(),
            usage: crate::UsageSnapshot::default(),
            models: crate::ModelsSnapshot::default(),
            environments: Vec::new(),
            terminals: Vec::new(),
            hosts: crate::HostsSnapshot::default(),
            chats: Vec::new(),
        })))),
        Frame::Message(Box::new(Message::Feed(Feed::Event(Event {
            seq: Seq(2),
            kind: EventKind::Pong {
                request: RequestId(1),
            },
        })))),
        Frame::Terminal(TerminalBytes {
            terminal: TerminalId(42),
            bytes: b"\x1b[31mred\x1b[0m\r\n\xff".to_vec(),
        }),
        Frame::Terminal(TerminalBytes {
            terminal: TerminalId(0),
            bytes: Vec::new(),
        }),
    ];
    let mut input = Cursor::new(encoded(&frames)?);
    for frame in &frames {
        assert_eq!(read_frame(&mut input)?.as_ref(), Some(frame));
    }
    assert!(read_frame(&mut input)?.is_none());
    Ok(())
}

#[test]
fn a_message_frame_carries_json() -> Result<(), FrameError> {
    let bytes = encoded(&[Frame::Message(Box::new(Message::Hello {
        version: ProtocolVersion(1),
    }))])?;
    assert_eq!(bytes[LENGTH_BYTES], FrameKind::Message as u8);
    assert_eq!(
        &bytes[LENGTH_BYTES + 1..],
        br#"{"type":"hello","version":1}"#
    );
    Ok(())
}

#[test]
fn a_stream_cut_inside_the_length_or_the_body_is_truncated() -> Result<(), FrameError> {
    let whole = encoded(&[Frame::Terminal(TerminalBytes {
        terminal: TerminalId(1),
        bytes: b"abc".to_vec(),
    })])?;
    for cut in [2, LENGTH_BYTES, whole.len() - 1] {
        let read = read_frame(&mut Cursor::new(&whole[..cut]));
        assert!(
            matches!(read, Err(FrameError::Truncated)),
            "cut at {cut}: {read:?}"
        );
    }
    Ok(())
}

#[test]
fn a_length_over_the_limit_is_refused_before_reading_the_body() {
    let header = (MAX_FRAME_BYTES + 1).to_be_bytes();
    let read = read_frame(&mut Cursor::new(header));
    assert!(matches!(read, Err(FrameError::TooLarge)));
}

#[test]
fn malformed_bodies_are_refused() {
    let cases: [(&[u8], ExpectedError); 4] = [
        (b"", |error| matches!(error, FrameError::Empty)),
        (b"\x09{}", |error| {
            matches!(error, FrameError::UnknownKind(9))
        }),
        (b"\x02\x00\x00\x01", |error| {
            matches!(error, FrameError::MissingTerminal)
        }),
        (b"\x01{\"type\":\"hello\"", |error| {
            matches!(error, FrameError::Json(_))
        }),
    ];
    for (body, expected) in cases {
        let read = read_frame(&mut Cursor::new(framed(body)));
        assert!(read.as_ref().is_err_and(expected), "{body:?}: {read:?}");
    }
}
