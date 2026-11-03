use std::io::{self, Read, Write};

pub(crate) const PROTOCOL_VERSION: u16 = crate::layout::MUX_PROTOCOL_VERSION;
pub(crate) const MAX_FRAME_BYTES: u32 = 256 * 1024;
pub(crate) const DATA_CHUNK_BYTES: usize = 32 * 1024;
pub(crate) const INITIAL_WINDOW_BYTES: u32 = 1024 * 1024;
pub(crate) const CONTROL_CHANNEL: u32 = 0;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const HEADER_BYTES: u32 = 5;

macro_rules! wire_tags {
    ($name:ident { $($variant:ident = $value:expr),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u8)]
        enum $name {
            $($variant = $value),+
        }

        impl TryFrom<u8> for $name {
            type Error = io::Error;

            fn try_from(byte: u8) -> io::Result<Self> {
                [$(Self::$variant),+]
                    .into_iter()
                    .find(|tag| *tag as u8 == byte)
                    .ok_or_else(malformed)
            }
        }
    };
}

wire_tags!(FrameKind {
    Hello = 1,
    Open = 2,
    Opened = 3,
    Refused = 4,
    Data = 5,
    Eof = 6,
    Window = 7,
    Exit = 8,
    Kill = 9,
    Close = 10,
    Resize = 11,
});

wire_tags!(OpenTag {
    Exec = 1,
    Sftp = 2,
    Connect = 3,
    Listen = 4,
    Accepted = 5,
    Pty = 6,
});

wire_tags!(StreamTag {
    Main = 0,
    Errors = 1,
});

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Stream {
    Main,
    Errors,
}

impl Stream {
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Main => 0,
            Self::Errors => 1,
        }
    }

    const fn byte(self) -> u8 {
        match self {
            Self::Main => StreamTag::Main as u8,
            Self::Errors => StreamTag::Errors as u8,
        }
    }

    fn from_byte(byte: u8) -> io::Result<Self> {
        Ok(match StreamTag::try_from(byte)? {
            StreamTag::Main => Self::Main,
            StreamTag::Errors => Self::Errors,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenRequest {
    Exec { script: String },
    Pty { script: String, size: TerminalSize },
    Sftp,
    Connect { port: u16 },
    Listen,
    Accepted { listener: u32 },
}

impl OpenRequest {
    const fn tag(&self) -> u8 {
        (match self {
            Self::Exec { .. } => OpenTag::Exec,
            Self::Pty { .. } => OpenTag::Pty,
            Self::Sftp => OpenTag::Sftp,
            Self::Connect { .. } => OpenTag::Connect,
            Self::Listen => OpenTag::Listen,
            Self::Accepted { .. } => OpenTag::Accepted,
        }) as u8
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Body {
    Hello { protocol: u16, version: String },
    Open(OpenRequest),
    Opened { port: Option<u16> },
    Refused { reason: String },
    Data { stream: Stream, bytes: Vec<u8> },
    Eof { stream: Stream },
    Window { stream: Stream, bytes: u32 },
    Exit { code: Option<i32> },
    Kill,
    Close,
    Resize(TerminalSize),
}

impl Body {
    const fn kind(&self) -> u8 {
        (match self {
            Self::Hello { .. } => FrameKind::Hello,
            Self::Open(_) => FrameKind::Open,
            Self::Opened { .. } => FrameKind::Opened,
            Self::Refused { .. } => FrameKind::Refused,
            Self::Data { .. } => FrameKind::Data,
            Self::Eof { .. } => FrameKind::Eof,
            Self::Window { .. } => FrameKind::Window,
            Self::Exit { .. } => FrameKind::Exit,
            Self::Kill => FrameKind::Kill,
            Self::Close => FrameKind::Close,
            Self::Resize(_) => FrameKind::Resize,
        }) as u8
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    pub(crate) channel: u32,
    pub(crate) body: Body,
}

impl Frame {
    pub(crate) const fn new(channel: u32, body: Body) -> Self {
        Self { channel, body }
    }

    pub(crate) fn encode(&self) -> io::Result<Vec<u8>> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&self.channel.to_be_bytes());
        payload.push(self.body.kind());
        match &self.body {
            Body::Hello { protocol, version } => {
                payload.extend_from_slice(&protocol.to_be_bytes());
                put_text(&mut payload, version)?;
            }
            Body::Open(request) => {
                payload.push(request.tag());
                match request {
                    OpenRequest::Exec { script } => put_bytes(&mut payload, script.as_bytes())?,
                    OpenRequest::Pty { script, size } => {
                        put_size(&mut payload, *size);
                        put_bytes(&mut payload, script.as_bytes())?;
                    }
                    OpenRequest::Connect { port } => payload.extend_from_slice(&port.to_be_bytes()),
                    OpenRequest::Accepted { listener } => {
                        payload.extend_from_slice(&listener.to_be_bytes());
                    }
                    OpenRequest::Sftp | OpenRequest::Listen => {}
                }
            }
            Body::Opened { port } => {
                if let Some(port) = port {
                    payload.extend_from_slice(&port.to_be_bytes());
                }
            }
            Body::Refused { reason } => put_text(&mut payload, reason)?,
            Body::Data { stream, bytes } => {
                payload.push(stream.byte());
                payload.extend_from_slice(bytes);
            }
            Body::Eof { stream } => payload.push(stream.byte()),
            Body::Window { stream, bytes } => {
                payload.push(stream.byte());
                payload.extend_from_slice(&bytes.to_be_bytes());
            }
            Body::Exit { code } => {
                if let Some(code) = code {
                    payload.extend_from_slice(&code.to_be_bytes());
                }
            }
            Body::Resize(size) => put_size(&mut payload, *size),
            Body::Kill | Body::Close => {}
        }
        let len = u32::try_from(payload.len()).map_err(|_| too_large())?;
        if len > MAX_FRAME_BYTES {
            return Err(too_large());
        }
        let mut framed = len.to_be_bytes().to_vec();
        framed.extend(payload);
        Ok(framed)
    }

    pub(crate) fn write_to(&self, out: &mut impl Write) -> io::Result<()> {
        out.write_all(&self.encode()?)?;
        out.flush()
    }

    pub(crate) fn read_from(input: &mut impl Read) -> io::Result<Option<Self>> {
        let mut len = [0; 4];
        if !read_exact_or_end(input, &mut len)? {
            return Ok(None);
        }
        let len = u32::from_be_bytes(len);
        if !(HEADER_BYTES..=MAX_FRAME_BYTES).contains(&len) {
            return Err(malformed());
        }
        let mut body = vec![0; usize::try_from(len).map_err(|_| malformed())?];
        input.read_exact(&mut body)?;
        Self::decode(&body).map(Some)
    }

    fn decode(raw: &[u8]) -> io::Result<Self> {
        let mut cursor = Cursor { raw, at: 0 };
        let channel = cursor.u32()?;
        let kind = FrameKind::try_from(cursor.u8()?)?;
        let body = match kind {
            FrameKind::Hello => Body::Hello {
                protocol: cursor.u16()?,
                version: cursor.text()?,
            },
            FrameKind::Open => Body::Open(match OpenTag::try_from(cursor.u8()?)? {
                OpenTag::Exec => OpenRequest::Exec {
                    script: String::from_utf8(cursor.bytes()?).map_err(|_| malformed())?,
                },
                OpenTag::Pty => OpenRequest::Pty {
                    size: cursor.size()?,
                    script: String::from_utf8(cursor.bytes()?).map_err(|_| malformed())?,
                },
                OpenTag::Sftp => OpenRequest::Sftp,
                OpenTag::Connect => OpenRequest::Connect {
                    port: cursor.u16()?,
                },
                OpenTag::Listen => OpenRequest::Listen,
                OpenTag::Accepted => OpenRequest::Accepted {
                    listener: cursor.u32()?,
                },
            }),
            FrameKind::Opened => Body::Opened {
                port: if cursor.is_done() {
                    None
                } else {
                    Some(cursor.u16()?)
                },
            },
            FrameKind::Refused => Body::Refused {
                reason: cursor.text()?,
            },
            FrameKind::Data => Body::Data {
                stream: Stream::from_byte(cursor.u8()?)?,
                bytes: cursor.rest().to_vec(),
            },
            FrameKind::Eof => Body::Eof {
                stream: Stream::from_byte(cursor.u8()?)?,
            },
            FrameKind::Window => Body::Window {
                stream: Stream::from_byte(cursor.u8()?)?,
                bytes: cursor.u32()?,
            },
            FrameKind::Exit => Body::Exit {
                code: if cursor.is_done() {
                    None
                } else {
                    Some(cursor.i32()?)
                },
            },
            FrameKind::Kill => Body::Kill,
            FrameKind::Close => Body::Close,
            FrameKind::Resize => Body::Resize(cursor.size()?),
        };
        Ok(Self { channel, body })
    }
}

fn read_exact_or_end(input: &mut impl Read, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match input.read(&mut buf[filled..]) {
            Ok(0) if filled == 0 => return Ok(false),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}

fn put_size(payload: &mut Vec<u8>, size: TerminalSize) {
    payload.extend_from_slice(&size.cols.to_be_bytes());
    payload.extend_from_slice(&size.rows.to_be_bytes());
}

fn put_bytes(payload: &mut Vec<u8>, bytes: &[u8]) -> io::Result<()> {
    let len = u32::try_from(bytes.len()).map_err(|_| too_large())?;
    payload.extend_from_slice(&len.to_be_bytes());
    payload.extend_from_slice(bytes);
    Ok(())
}

fn put_text(payload: &mut Vec<u8>, text: &str) -> io::Result<()> {
    let end = text
        .char_indices()
        .map(|(at, c)| at + c.len_utf8())
        .take_while(|end| *end <= MAX_TEXT_BYTES)
        .last()
        .unwrap_or(0);
    put_bytes(payload, &text.as_bytes()[..end])
}

struct Cursor<'a> {
    raw: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn take(&mut self, count: usize) -> io::Result<&[u8]> {
        let end = self.at.checked_add(count).ok_or_else(malformed)?;
        let slice = self.raw.get(self.at..end).ok_or_else(malformed)?;
        self.at = end;
        Ok(slice)
    }

    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> io::Result<u16> {
        let bytes: [u8; 2] = self.take(2)?.try_into().map_err(|_| malformed())?;
        Ok(u16::from_be_bytes(bytes))
    }

    fn size(&mut self) -> io::Result<TerminalSize> {
        Ok(TerminalSize {
            cols: self.u16()?,
            rows: self.u16()?,
        })
    }

    fn u32(&mut self) -> io::Result<u32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| malformed())?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn i32(&mut self) -> io::Result<i32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| malformed())?;
        Ok(i32::from_be_bytes(bytes))
    }

    fn bytes(&mut self) -> io::Result<Vec<u8>> {
        let len = usize::try_from(self.u32()?).map_err(|_| malformed())?;
        Ok(self.take(len)?.to_vec())
    }

    fn text(&mut self) -> io::Result<String> {
        Ok(String::from_utf8_lossy(&self.bytes()?).into_owned())
    }

    fn rest(&mut self) -> &[u8] {
        let rest = &self.raw[self.at..];
        self.at = self.raw.len();
        rest
    }

    const fn is_done(&self) -> bool {
        self.at >= self.raw.len()
    }
}

fn malformed() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "malformed multiplexer frame")
}

fn too_large() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "multiplexer frame too large")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(frame: &Frame) -> io::Result<Frame> {
        let bytes = frame.encode()?;
        Frame::read_from(&mut bytes.as_slice())?.ok_or_else(malformed)
    }

    #[test]
    fn every_frame_kind_roundtrips() -> io::Result<()> {
        let bodies = [
            Body::Hello {
                protocol: PROTOCOL_VERSION,
                version: "0.2.0".to_owned(),
            },
            Body::Open(OpenRequest::Exec {
                script: "printf 'é'".to_owned(),
            }),
            Body::Open(OpenRequest::Sftp),
            Body::Open(OpenRequest::Pty {
                script: "exec \"${SHELL:-sh}\" -l".to_owned(),
                size: TerminalSize {
                    cols: 132,
                    rows: 43,
                },
            }),
            Body::Resize(TerminalSize { cols: 80, rows: 24 }),
            Body::Open(OpenRequest::Connect { port: 4096 }),
            Body::Open(OpenRequest::Listen),
            Body::Open(OpenRequest::Accepted { listener: 7 }),
            Body::Opened { port: None },
            Body::Opened { port: Some(41_000) },
            Body::Refused {
                reason: "no".to_owned(),
            },
            Body::Data {
                stream: Stream::Errors,
                bytes: vec![0, 1, 255],
            },
            Body::Eof {
                stream: Stream::Main,
            },
            Body::Window {
                stream: Stream::Main,
                bytes: 65_536,
            },
            Body::Exit { code: Some(-1) },
            Body::Exit { code: None },
            Body::Kill,
            Body::Close,
        ];
        for body in bodies {
            let frame = Frame::new(42, body);
            assert_eq!(roundtrip(&frame)?, frame);
        }
        Ok(())
    }

    #[test]
    fn a_clean_end_between_frames_is_not_an_error() -> io::Result<()> {
        assert!(Frame::read_from(&mut [].as_slice())?.is_none());
        assert!(Frame::read_from(&mut [0, 0].as_slice()).is_err());
        Ok(())
    }

    #[test]
    fn oversized_and_unknown_frames_are_rejected() {
        let oversized = (MAX_FRAME_BYTES + 1).to_be_bytes();
        assert!(Frame::read_from(&mut oversized.as_slice()).is_err());
        let unknown = [0, 0, 0, 5, 0, 0, 0, 1, 99];
        assert!(Frame::read_from(&mut unknown.as_slice()).is_err());
        let data = Frame::new(
            1,
            Body::Data {
                stream: Stream::Main,
                bytes: vec![0; usize::try_from(MAX_FRAME_BYTES).unwrap_or(usize::MAX)],
            },
        );
        assert!(data.encode().is_err());
    }
}
