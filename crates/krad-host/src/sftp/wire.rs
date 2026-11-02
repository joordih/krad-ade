use std::io::{self, Read, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) const PROTOCOL_VERSION: u32 = 3;
pub(crate) const MAX_PACKET_BYTES: u32 = 256 * 1024;

const ATTR_SIZE: u32 = 0x0000_0001;
const ATTR_UIDGID: u32 = 0x0000_0002;
const ATTR_PERMISSIONS: u32 = 0x0000_0004;
const ATTR_ACMODTIME: u32 = 0x0000_0008;
const ATTR_EXTENDED: u32 = 0x8000_0000;

const FILE_TYPE_MASK: u32 = 0o170_000;
const FILE_TYPE_DIRECTORY: u32 = 0o040_000;
const FILE_TYPE_SYMLINK: u32 = 0o120_000;
const PERMISSION_BITS: u32 = 0o7777;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum PacketType {
    Init = 1,
    Version = 2,
    Open = 3,
    Close = 4,
    Read = 5,
    Write = 6,
    Lstat = 7,
    Opendir = 11,
    Readdir = 12,
    Remove = 13,
    Mkdir = 14,
    Rmdir = 15,
    Realpath = 16,
    Stat = 17,
    Rename = 18,
    Symlink = 20,
    Setstat = 9,
    Status = 101,
    Handle = 102,
    Data = 103,
    Name = 104,
    Attrs = 105,
    Extended = 200,
}

impl PacketType {
    pub(crate) fn from_byte(byte: u8) -> Option<Self> {
        [
            Self::Version,
            Self::Status,
            Self::Handle,
            Self::Data,
            Self::Name,
            Self::Attrs,
        ]
        .into_iter()
        .find(|kind| *kind as u8 == byte)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OpenFlags(pub(crate) u32);

impl OpenFlags {
    pub(crate) const READ: Self = Self(0x01);
    pub(crate) const WRITE_NEW: Self = Self(0x02 | 0x08 | 0x10);
    pub(crate) const CREATE_EXCLUSIVE: Self = Self(0x02 | 0x08 | 0x20);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusCode {
    Ok,
    Eof,
    NoSuchFile,
    PermissionDenied,
    Other(u32),
}

impl From<u32> for StatusCode {
    fn from(code: u32) -> Self {
        match code {
            0 => Self::Ok,
            1 => Self::Eof,
            2 => Self::NoSuchFile,
            3 => Self::PermissionDenied,
            other => Self::Other(other),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Directory,
    File,
    Symlink,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Attrs {
    pub(crate) size: Option<u64>,
    pub(crate) owner: Option<(u32, u32)>,
    pub(crate) permissions: Option<u32>,
    pub(crate) modified_secs: Option<u32>,
}

impl Attrs {
    pub(crate) fn kind(&self) -> FileKind {
        match self.permissions.map(|mode| mode & FILE_TYPE_MASK) {
            Some(FILE_TYPE_DIRECTORY) => FileKind::Directory,
            Some(FILE_TYPE_SYMLINK) => FileKind::Symlink,
            _ => FileKind::File,
        }
    }

    pub(crate) fn mode(&self) -> Option<u32> {
        self.permissions.map(|mode| mode & PERMISSION_BITS)
    }

    pub(crate) fn modified(&self) -> Option<SystemTime> {
        self.modified_secs
            .map(|secs| UNIX_EPOCH + Duration::from_secs(u64::from(secs)))
    }
}

#[derive(Default)]
pub(crate) struct Packet {
    body: Vec<u8>,
}

impl Packet {
    pub(crate) fn new(kind: PacketType) -> Self {
        Self {
            body: vec![kind as u8],
        }
    }

    pub(crate) fn request(kind: PacketType, id: u32) -> Self {
        let mut packet = Self::new(kind);
        packet.u32(id);
        packet
    }

    pub(crate) fn u32(&mut self, value: u32) -> &mut Self {
        self.body.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub(crate) fn u64(&mut self, value: u64) -> &mut Self {
        self.body.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub(crate) fn bytes(&mut self, value: &[u8]) -> io::Result<&mut Self> {
        self.u32(u32::try_from(value.len()).map_err(|_| too_large())?);
        self.body.extend_from_slice(value);
        Ok(self)
    }

    pub(crate) fn str(&mut self, value: &str) -> io::Result<&mut Self> {
        self.bytes(value.as_bytes())
    }

    pub(crate) fn empty_attrs(&mut self) -> &mut Self {
        self.u32(0)
    }

    pub(crate) fn mode_attrs(&mut self, mode: Option<u32>) -> &mut Self {
        match mode {
            Some(mode) => self.u32(ATTR_PERMISSIONS).u32(mode),
            None => self.empty_attrs(),
        }
    }

    pub(crate) fn send(&self, out: &mut impl Write) -> io::Result<()> {
        let len = u32::try_from(self.body.len()).map_err(|_| too_large())?;
        out.write_all(&len.to_be_bytes())?;
        out.write_all(&self.body)?;
        out.flush()
    }
}

pub(crate) struct Reply {
    pub(crate) kind: PacketType,
    pub(crate) id: u32,
    body: Vec<u8>,
    at: usize,
}

impl Reply {
    pub(crate) fn receive(input: &mut impl Read) -> io::Result<Self> {
        let mut len = [0; 4];
        input.read_exact(&mut len)?;
        let len = u32::from_be_bytes(len);
        if len == 0 || len > MAX_PACKET_BYTES {
            return Err(malformed());
        }
        let mut body = vec![0; usize::try_from(len).map_err(|_| malformed())?];
        input.read_exact(&mut body)?;
        let kind = PacketType::from_byte(body[0]).ok_or_else(malformed)?;
        let mut reply = Self {
            kind,
            id: 0,
            body,
            at: 1,
        };
        if kind != PacketType::Version {
            reply.id = reply.u32()?;
        }
        Ok(reply)
    }

    fn take(&mut self, count: usize) -> io::Result<&[u8]> {
        let end = self.at.checked_add(count).ok_or_else(malformed)?;
        let slice = self.body.get(self.at..end).ok_or_else(malformed)?;
        self.at = end;
        Ok(slice)
    }

    pub(crate) fn u32(&mut self) -> io::Result<u32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| malformed())?;
        Ok(u32::from_be_bytes(bytes))
    }

    pub(crate) fn u64(&mut self) -> io::Result<u64> {
        let bytes: [u8; 8] = self.take(8)?.try_into().map_err(|_| malformed())?;
        Ok(u64::from_be_bytes(bytes))
    }

    pub(crate) fn bytes(&mut self) -> io::Result<Vec<u8>> {
        let len = usize::try_from(self.u32()?).map_err(|_| malformed())?;
        Ok(self.take(len)?.to_vec())
    }

    pub(crate) fn string(&mut self) -> io::Result<String> {
        Ok(String::from_utf8_lossy(&self.bytes()?).into_owned())
    }

    pub(crate) fn has_more(&self) -> bool {
        self.at < self.body.len()
    }

    pub(crate) fn attrs(&mut self) -> io::Result<Attrs> {
        let flags = self.u32()?;
        let mut attrs = Attrs::default();
        if flags & ATTR_SIZE != 0 {
            attrs.size = Some(self.u64()?);
        }
        if flags & ATTR_UIDGID != 0 {
            attrs.owner = Some((self.u32()?, self.u32()?));
        }
        if flags & ATTR_PERMISSIONS != 0 {
            attrs.permissions = Some(self.u32()?);
        }
        if flags & ATTR_ACMODTIME != 0 {
            self.u32()?;
            attrs.modified_secs = Some(self.u32()?);
        }
        if flags & ATTR_EXTENDED != 0 {
            for _ in 0..self.u32()? {
                self.bytes()?;
                self.bytes()?;
            }
        }
        Ok(attrs)
    }
}

fn malformed() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "malformed SFTP packet")
}

fn too_large() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "SFTP field too large")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply_from(body: Vec<u8>) -> io::Result<Reply> {
        let mut framed = u32::try_from(body.len())
            .map_err(|_| too_large())?
            .to_be_bytes()
            .to_vec();
        framed.extend(body);
        Reply::receive(&mut framed.as_slice())
    }

    #[test]
    fn packets_are_framed_with_their_length() -> io::Result<()> {
        let mut out = Vec::new();
        Packet::request(PacketType::Stat, 7)
            .str("/tmp")?
            .send(&mut out)?;
        assert_eq!(
            out,
            [0, 0, 0, 13, 17, 0, 0, 0, 7, 0, 0, 0, 4, b'/', b't', b'm', b'p']
        );
        Ok(())
    }

    #[test]
    fn attributes_read_size_owner_kind_and_mtime_and_skip_the_rest() -> io::Result<()> {
        let mut body = vec![PacketType::Attrs as u8];
        body.extend(9_u32.to_be_bytes());
        let flags = ATTR_SIZE | ATTR_UIDGID | ATTR_PERMISSIONS | ATTR_ACMODTIME | ATTR_EXTENDED;
        body.extend(flags.to_be_bytes());
        body.extend(42_u64.to_be_bytes());
        body.extend([0, 0, 3, 232, 0, 0, 3, 232]);
        body.extend((FILE_TYPE_DIRECTORY | 0o755).to_be_bytes());
        body.extend(1_u32.to_be_bytes());
        body.extend(1_700_000_000_u32.to_be_bytes());
        body.extend(1_u32.to_be_bytes());
        body.extend([0, 0, 0, 1, b'k', 0, 0, 0, 1, b'v']);
        let mut reply = reply_from(body)?;
        assert_eq!(reply.kind, PacketType::Attrs);
        assert_eq!(reply.id, 9);
        let attrs = reply.attrs()?;
        assert_eq!(attrs.size, Some(42));
        assert_eq!(attrs.owner, Some((1000, 1000)));
        assert_eq!(attrs.mode(), Some(0o755));
        assert_eq!(attrs.kind(), FileKind::Directory);
        assert_eq!(attrs.modified_secs, Some(1_700_000_000));
        assert!(!reply.has_more());
        Ok(())
    }

    #[test]
    fn truncated_and_oversized_packets_are_rejected() {
        assert!(reply_from(vec![PacketType::Status as u8, 0, 0]).is_err());
        let oversized = (MAX_PACKET_BYTES + 1).to_be_bytes();
        assert!(Reply::receive(&mut oversized.as_slice()).is_err());
    }
}
