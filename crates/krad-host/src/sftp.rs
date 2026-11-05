mod channel;
mod names;
mod save;
mod server;
mod transfer;
mod wire;

pub use names::join;
pub(crate) use names::{file_name, parent};
pub use save::Saved;
pub(crate) use server::serve;
pub use wire::FileKind;

use std::io::{self, Read, Write};
use std::time::Duration;
use thiserror::Error;

use crate::metadata::Metadata;
use channel::Channel;
use wire::{Attrs, Message, OpenFlags, Packet, PacketType, StatusCode, PROTOCOL_VERSION};

const POSIX_RENAME_EXTENSION: &str = "posix-rename@openssh.com";
const CURRENT_DIR: &str = ".";
const PARENT_DIR: &str = "..";

#[derive(Debug, Error)]
pub enum SftpError {
    #[error("the SFTP channel failed: {0}")]
    Io(#[from] io::Error),
    #[error("no such file or folder: {0}")]
    NotFound(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("the host refused {path}: {message}")]
    Refused { path: String, message: String },
    #[error("the host sent an unexpected SFTP reply")]
    Protocol,
    #[error("the host did not answer the SFTP request in time")]
    TimedOut,
}

impl SftpError {
    #[must_use]
    pub const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }
}

impl From<Attrs> for Metadata {
    fn from(attrs: Attrs) -> Self {
        Self {
            kind: attrs.kind(),
            len: attrs.size.unwrap_or_default(),
            modified: attrs.modified(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub metadata: Metadata,
    pub symlink: bool,
}

pub struct Sftp {
    channel: Channel,
    posix_rename: bool,
}

impl Sftp {
    /// # Errors
    pub fn handshake(
        mut input: impl Read + Send + 'static,
        mut output: impl Write + Send + 'static,
        timeout: Duration,
    ) -> Result<Self, SftpError> {
        Packet::new(PacketType::Init)
            .u32(PROTOCOL_VERSION)
            .send(&mut output)?;
        let mut reply = Message::receive(&mut input)?;
        if reply.kind != PacketType::Version {
            return Err(SftpError::Protocol);
        }
        reply.u32()?;
        let mut posix_rename = false;
        while reply.has_more() {
            let name = reply.string()?;
            reply.bytes()?;
            posix_rename |= name == POSIX_RENAME_EXTENSION;
        }
        Ok(Self {
            channel: Channel::start(input, output, timeout)?,
            posix_rename,
        })
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.channel.is_closed()
    }

    fn call(
        &self,
        kind: PacketType,
        fill: impl FnOnce(&mut Packet) -> io::Result<()>,
    ) -> Result<Message, SftpError> {
        self.channel.call(kind, fill)
    }

    fn path_call(&self, kind: PacketType, path: &str) -> Result<Message, SftpError> {
        self.call(kind, |packet| packet.str(path).map(drop))
    }

    fn expect_ok(mut reply: Message, path: &str) -> Result<(), SftpError> {
        if reply.kind != PacketType::Status {
            return Err(SftpError::Protocol);
        }
        match status(&mut reply, path)? {
            StatusCode::Ok => Ok(()),
            _ => Err(SftpError::Protocol),
        }
    }

    fn expect_attrs(mut reply: Message, path: &str) -> Result<Attrs, SftpError> {
        match reply.kind {
            PacketType::Attrs => Ok(reply.attrs()?),
            PacketType::Status => {
                status(&mut reply, path)?;
                Err(SftpError::Protocol)
            }
            _ => Err(SftpError::Protocol),
        }
    }

    fn handle(
        &self,
        kind: PacketType,
        path: &str,
        fill: impl FnOnce(&mut Packet) -> io::Result<()>,
    ) -> Result<Vec<u8>, SftpError> {
        let mut reply = self.call(kind, |packet| {
            packet.str(path)?;
            fill(packet)
        })?;
        match reply.kind {
            PacketType::Handle => Ok(reply.bytes()?),
            PacketType::Status => {
                status(&mut reply, path)?;
                Err(SftpError::Protocol)
            }
            _ => Err(SftpError::Protocol),
        }
    }

    fn open(&self, path: &str, flags: OpenFlags, mode: Option<u32>) -> Result<Vec<u8>, SftpError> {
        self.handle(PacketType::Open, path, |packet| {
            packet.u32(flags.0).mode_attrs(mode);
            Ok(())
        })
    }

    fn close(&self, handle: &[u8], path: &str) -> Result<(), SftpError> {
        let reply = self.call(PacketType::Close, |packet| packet.bytes(handle).map(drop))?;
        Self::expect_ok(reply, path)
    }

    fn attrs(&self, path: &str) -> Result<Attrs, SftpError> {
        Self::expect_attrs(self.path_call(PacketType::Stat, path)?, path)
    }

    fn link_attrs(&self, path: &str) -> Result<Attrs, SftpError> {
        Self::expect_attrs(self.path_call(PacketType::Lstat, path)?, path)
    }

    /// # Errors
    pub fn stat(&self, path: &str) -> Result<Metadata, SftpError> {
        self.attrs(path).map(Into::into)
    }

    /// # Errors
    pub fn lstat(&self, path: &str) -> Result<Metadata, SftpError> {
        self.link_attrs(path).map(Into::into)
    }

    /// # Errors
    pub fn create_new(&self, path: &str) -> Result<(), SftpError> {
        let handle = self.open(path, OpenFlags::CREATE_EXCLUSIVE, None)?;
        self.close(&handle, path)
    }

    /// # Errors
    pub fn realpath(&self, path: &str) -> Result<String, SftpError> {
        let mut reply = self.path_call(PacketType::Realpath, path)?;
        match reply.kind {
            PacketType::Name => {
                if reply.u32()? == 0 {
                    return Err(SftpError::Protocol);
                }
                Ok(reply.string()?)
            }
            PacketType::Status => {
                status(&mut reply, path)?;
                Err(SftpError::Protocol)
            }
            _ => Err(SftpError::Protocol),
        }
    }

    /// # Errors
    pub fn read_dir(&self, path: &str) -> Result<Vec<RemoteEntry>, SftpError> {
        let handle = self.handle(PacketType::Opendir, path, |_| Ok(()))?;
        let entries = self.read_entries(&handle, path);
        self.close(&handle, path)?;
        let mut entries = entries?;
        let links: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.metadata.kind == FileKind::Symlink)
            .map(|(index, _)| index)
            .collect();
        let followed: Vec<_> = links
            .iter()
            .map(|&index| {
                let target = join(path, &entries[index].name);
                self.channel
                    .send(PacketType::Stat, |packet| packet.str(&target).map(drop))
            })
            .collect();
        for (index, pending) in links.into_iter().zip(followed) {
            entries[index].symlink = true;
            let Ok(pending) = pending else {
                continue;
            };
            if let Ok(attrs) = pending
                .wait()
                .and_then(|reply| Self::expect_attrs(reply, path))
            {
                entries[index].metadata = attrs.into();
            }
        }
        Ok(entries)
    }

    fn read_entries(&self, handle: &[u8], path: &str) -> Result<Vec<RemoteEntry>, SftpError> {
        let mut entries = Vec::new();
        loop {
            let mut reply =
                self.call(PacketType::Readdir, |packet| packet.bytes(handle).map(drop))?;
            match reply.kind {
                PacketType::Name => {
                    for _ in 0..reply.u32()? {
                        let name = reply.string()?;
                        reply.string()?;
                        let metadata = reply.attrs()?.into();
                        if name != CURRENT_DIR && name != PARENT_DIR {
                            entries.push(RemoteEntry {
                                name,
                                metadata,
                                symlink: false,
                            });
                        }
                    }
                }
                PacketType::Status => match status(&mut reply, path)? {
                    StatusCode::Eof => return Ok(entries),
                    _ => return Err(SftpError::Protocol),
                },
                _ => return Err(SftpError::Protocol),
            }
        }
    }

    /// # Errors
    pub fn read(&self, path: &str, limit: u64) -> Result<Vec<u8>, SftpError> {
        self.read_range(path, 0, limit)
    }

    /// # Errors
    pub fn read_range(&self, path: &str, from: u64, limit: u64) -> Result<Vec<u8>, SftpError> {
        let handle = self.open(path, OpenFlags::READ, None)?;
        let bytes = self.read_handle(&handle, path, from, limit);
        self.close(&handle, path)?;
        bytes
    }

    /// # Errors
    pub fn write(&self, path: &str, bytes: &[u8]) -> Result<(), SftpError> {
        self.write_with_mode(path, bytes, None)
    }

    /// # Errors
    pub fn write_with_mode(
        &self,
        path: &str,
        bytes: &[u8],
        mode: Option<u32>,
    ) -> Result<(), SftpError> {
        let handle = self.open(path, OpenFlags::WRITE_NEW, mode)?;
        let written = self.write_handle(&handle, path, bytes);
        self.close(&handle, path)?;
        written
    }

    /// # Errors
    pub fn mkdir(&self, path: &str) -> Result<(), SftpError> {
        self.mkdir_with_mode(path, None)
    }

    /// # Errors
    pub fn mkdir_with_mode(&self, path: &str, mode: Option<u32>) -> Result<(), SftpError> {
        let reply = self.call(PacketType::Mkdir, |packet| {
            packet.str(path)?.mode_attrs(mode);
            Ok(())
        })?;
        Self::expect_ok(reply, path)
    }

    /// # Errors
    pub fn set_permissions(&self, path: &str, mode: u32) -> Result<(), SftpError> {
        let reply = self.call(PacketType::Setstat, |packet| {
            packet.str(path)?.mode_attrs(Some(mode));
            Ok(())
        })?;
        Self::expect_ok(reply, path)
    }

    /// # Errors
    pub fn remove_file(&self, path: &str) -> Result<(), SftpError> {
        Self::expect_ok(self.path_call(PacketType::Remove, path)?, path)
    }

    /// # Errors
    pub fn remove_dir(&self, path: &str) -> Result<(), SftpError> {
        Self::expect_ok(self.path_call(PacketType::Rmdir, path)?, path)
    }

    /// # Errors
    pub fn rename(&self, from: &str, to: &str) -> Result<(), SftpError> {
        let reply = if self.posix_rename {
            self.call(PacketType::Extended, |packet| {
                packet.str(POSIX_RENAME_EXTENSION)?.str(from)?.str(to)?;
                Ok(())
            })?
        } else {
            self.call(PacketType::Rename, |packet| {
                packet.str(from)?.str(to)?;
                Ok(())
            })?
        };
        Self::expect_ok(reply, from)
    }

    /// # Errors
    pub fn symlink(&self, target: &str, link: &str) -> Result<(), SftpError> {
        let reply = self.call(PacketType::Symlink, |packet| {
            packet.str(target)?.str(link)?;
            Ok(())
        })?;
        Self::expect_ok(reply, link)
    }
}

fn status(reply: &mut Message, path: &str) -> Result<StatusCode, SftpError> {
    let code = StatusCode::from(reply.u32()?);
    let message = if reply.has_more() {
        reply.string()?
    } else {
        String::new()
    };
    match code {
        StatusCode::Ok | StatusCode::Eof => Ok(code),
        StatusCode::NoSuchFile => Err(SftpError::NotFound(path.to_owned())),
        StatusCode::PermissionDenied => Err(SftpError::PermissionDenied(path.to_owned())),
        StatusCode::Failure
        | StatusCode::BadMessage
        | StatusCode::OpUnsupported
        | StatusCode::Other(_) => Err(SftpError::Refused {
            path: path.to_owned(),
            message,
        }),
    }
}
