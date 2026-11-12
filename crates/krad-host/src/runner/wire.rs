mod codec;
#[cfg(test)]
mod tests;

use krad_domain::{CloudProcessId, JournalOffset, Pairing};
use std::io::{self, Read, Write};
use std::path::PathBuf;

use self::codec::{
    malformed, read_bytes, read_code, read_count, read_id, read_offset, read_optional_size,
    read_parsed, read_run, read_size, read_text, read_u8, write_bytes, write_code, write_len,
    write_offset, write_optional_size, write_run, write_size, write_text, write_u8, MAX_DATA_BYTES,
};
use super::status::CloudProcessStatus;
use crate::mux::TerminalSize;

pub(crate) const PROTOCOL_VERSION: u8 = 2;

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

wire_tags!(RequestTag {
    Open = 1,
    Stop = 2,
    List = 3,
    Resize = 4
});
wire_tags!(ReplyTag {
    Accepted = 1,
    Refused = 2,
    Done = 3,
    Listed = 4,
    PastEnd = 5
});
wire_tags!(FrameTag { Data = 1, Exit = 2 });
wire_tags!(LaunchTag { Attach = 0, Start = 1 });

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudLaunch {
    pub cwd: PathBuf,
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub terminal: Option<TerminalSize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Request {
    Open {
        id: CloudProcessId,
        from: JournalOffset,
        launch: Option<CloudLaunch>,
    },
    Stop {
        id: CloudProcessId,
    },
    List,
    Resize {
        id: CloudProcessId,
        size: TerminalSize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Envelope {
    pub(crate) pairing: Pairing,
    pub(crate) request: Request,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reply {
    Accepted { end: JournalOffset },
    Refused(String),
    PastEnd { end: JournalOffset },
    Done,
    Listed(Vec<CloudProcessStatus>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Frame {
    Data(Vec<u8>),
    Exit(Option<i32>),
}

impl Envelope {
    pub(crate) fn write(&self, out: &mut impl Write) -> io::Result<()> {
        write_u8(out, PROTOCOL_VERSION)?;
        write_text(out, self.pairing.install.as_str())?;
        write_text(out, self.pairing.token.expose())?;
        self.request.write(out)?;
        out.flush()
    }

    pub(crate) fn read(input: &mut impl Read) -> io::Result<Self> {
        let version = read_u8(input)?;
        if version != PROTOCOL_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "the cloud runner speaks protocol {PROTOCOL_VERSION} but the client sent \
                     {version}; update the host CLI on the runner"
                ),
            ));
        }
        let install = read_parsed(input)?;
        let token = read_parsed(input)?;
        Ok(Self {
            pairing: Pairing { install, token },
            request: Request::read(input)?,
        })
    }
}

impl Request {
    fn write(&self, out: &mut impl Write) -> io::Result<()> {
        match self {
            Self::Open { id, from, launch } => {
                write_u8(out, RequestTag::Open as u8)?;
                write_text(out, id.as_str())?;
                write_offset(out, *from)?;
                match launch {
                    None => write_u8(out, LaunchTag::Attach as u8),
                    Some(launch) => {
                        write_u8(out, LaunchTag::Start as u8)?;
                        write_launch(out, launch)
                    }
                }
            }
            Self::Stop { id } => {
                write_u8(out, RequestTag::Stop as u8)?;
                write_text(out, id.as_str())
            }
            Self::List => write_u8(out, RequestTag::List as u8),
            Self::Resize { id, size } => {
                write_u8(out, RequestTag::Resize as u8)?;
                write_text(out, id.as_str())?;
                write_size(out, *size)
            }
        }
    }

    fn read(input: &mut impl Read) -> io::Result<Self> {
        Ok(match RequestTag::try_from(read_u8(input)?)? {
            RequestTag::Open => {
                let id = read_id(input)?;
                let from = read_offset(input)?;
                let launch = match LaunchTag::try_from(read_u8(input)?)? {
                    LaunchTag::Attach => None,
                    LaunchTag::Start => Some(read_launch(input)?),
                };
                Self::Open { id, from, launch }
            }
            RequestTag::Stop => Self::Stop {
                id: read_id(input)?,
            },
            RequestTag::List => Self::List,
            RequestTag::Resize => Self::Resize {
                id: read_id(input)?,
                size: read_size(input)?,
            },
        })
    }
}

impl Reply {
    pub(crate) fn write(&self, out: &mut impl Write) -> io::Result<()> {
        match self {
            Self::Accepted { end } => {
                write_u8(out, ReplyTag::Accepted as u8)?;
                write_offset(out, *end)?;
            }
            Self::Refused(reason) => {
                write_u8(out, ReplyTag::Refused as u8)?;
                write_text(out, reason)?;
            }
            Self::PastEnd { end } => {
                write_u8(out, ReplyTag::PastEnd as u8)?;
                write_offset(out, *end)?;
            }
            Self::Done => write_u8(out, ReplyTag::Done as u8)?,
            Self::Listed(processes) => {
                write_u8(out, ReplyTag::Listed as u8)?;
                write_len(out, processes.len())?;
                for process in processes {
                    write_text(out, process.id.as_str())?;
                    write_run(out, process.run)?;
                    write_offset(out, process.end)?;
                }
            }
        }
        out.flush()
    }

    pub(crate) fn read(input: &mut impl Read) -> io::Result<Self> {
        Ok(match ReplyTag::try_from(read_u8(input)?)? {
            ReplyTag::Accepted => Self::Accepted {
                end: read_offset(input)?,
            },
            ReplyTag::Refused => Self::Refused(read_text(input)?),
            ReplyTag::PastEnd => Self::PastEnd {
                end: read_offset(input)?,
            },
            ReplyTag::Done => Self::Done,
            ReplyTag::Listed => Self::Listed(
                (0..read_count(input)?)
                    .map(|_| {
                        Ok(CloudProcessStatus {
                            id: read_id(input)?,
                            run: read_run(input)?,
                            end: read_offset(input)?,
                        })
                    })
                    .collect::<io::Result<_>>()?,
            ),
        })
    }
}

impl Frame {
    pub(crate) fn write(&self, out: &mut impl Write) -> io::Result<()> {
        match self {
            Self::Data(bytes) => {
                write_u8(out, FrameTag::Data as u8)?;
                write_bytes(out, bytes)?;
            }
            Self::Exit(code) => {
                write_u8(out, FrameTag::Exit as u8)?;
                write_code(out, *code)?;
            }
        }
        out.flush()
    }

    pub(crate) fn read(input: &mut impl Read) -> io::Result<Self> {
        Ok(match FrameTag::try_from(read_u8(input)?)? {
            FrameTag::Data => Self::Data(read_bytes(input, MAX_DATA_BYTES)?),
            FrameTag::Exit => Self::Exit(read_code(input)?),
        })
    }
}

fn write_launch(out: &mut impl Write, launch: &CloudLaunch) -> io::Result<()> {
    write_text(out, &launch.cwd.to_string_lossy())?;
    write_text(out, &launch.program)?;
    write_len(out, launch.args.len())?;
    for arg in &launch.args {
        write_text(out, arg)?;
    }
    write_len(out, launch.env.len())?;
    for (name, value) in &launch.env {
        write_text(out, name)?;
        write_text(out, value)?;
    }
    write_optional_size(out, launch.terminal)
}

fn read_launch(input: &mut impl Read) -> io::Result<CloudLaunch> {
    let cwd = PathBuf::from(read_text(input)?);
    let program = read_text(input)?;
    let args = (0..read_count(input)?)
        .map(|_| read_text(input))
        .collect::<io::Result<_>>()?;
    let env = (0..read_count(input)?)
        .map(|_| Ok((read_text(input)?, read_text(input)?)))
        .collect::<io::Result<_>>()?;
    let terminal = read_optional_size(input)?;
    Ok(CloudLaunch {
        cwd,
        program,
        args,
        env,
        terminal,
    })
}
