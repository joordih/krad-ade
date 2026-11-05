mod attrs;
mod status;

pub(crate) use attrs::Attrs;
pub use attrs::FileKind;
pub(crate) use status::StatusCode;

use std::io::{self, Read, Write};

pub(crate) const PROTOCOL_VERSION: u32 = 3;
pub(crate) const MAX_PACKET_BYTES: u32 = 256 * 1024;

const LENGTH_BYTES: usize = 4;
const KIND_BYTES: usize = 1;
const ID_BYTES: usize = 4;

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
    Fstat = 8,
    Setstat = 9,
    Fsetstat = 10,
    Opendir = 11,
    Readdir = 12,
    Remove = 13,
    Mkdir = 14,
    Rmdir = 15,
    Realpath = 16,
    Stat = 17,
    Rename = 18,
    Readlink = 19,
    Symlink = 20,
    Status = 101,
    Handle = 102,
    Data = 103,
    Name = 104,
    Attrs = 105,
    Extended = 200,
}

impl PacketType {
    const ALL: [Self; 25] = [
        Self::Init,
        Self::Version,
        Self::Open,
        Self::Close,
        Self::Read,
        Self::Write,
        Self::Lstat,
        Self::Fstat,
        Self::Setstat,
        Self::Fsetstat,
        Self::Opendir,
        Self::Readdir,
        Self::Remove,
        Self::Mkdir,
        Self::Rmdir,
        Self::Realpath,
        Self::Stat,
        Self::Rename,
        Self::Readlink,
        Self::Symlink,
        Self::Status,
        Self::Handle,
        Self::Data,
        Self::Name,
        Self::Attrs,
    ];

    pub(crate) fn from_byte(byte: u8) -> Option<Self> {
        Self::ALL
            .into_iter()
            .chain([Self::Extended])
            .find(|kind| *kind as u8 == byte)
    }

    const fn has_id(self) -> bool {
        !matches!(self, Self::Init | Self::Version)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OpenFlags(pub(crate) u32);

impl OpenFlags {
    pub(crate) const READ: Self = Self(0x01);
    pub(crate) const WRITE: Self = Self(0x02);
    pub(crate) const APPEND: Self = Self(0x04);
    pub(crate) const CREATE: Self = Self(0x08);
    pub(crate) const TRUNCATE: Self = Self(0x10);
    pub(crate) const EXCLUSIVE: Self = Self(0x20);
    pub(crate) const WRITE_NEW: Self = Self(Self::WRITE.0 | Self::CREATE.0 | Self::TRUNCATE.0);
    pub(crate) const CREATE_EXCLUSIVE: Self =
        Self(Self::WRITE.0 | Self::CREATE.0 | Self::EXCLUSIVE.0);

    pub(crate) const fn contains(self, flags: Self) -> bool {
        self.0 & flags.0 == flags.0
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

    pub(crate) fn send(&self, out: &mut impl Write) -> io::Result<()> {
        let len = u32::try_from(self.body.len()).map_err(|_| too_large())?;
        out.write_all(&len.to_be_bytes())?;
        out.write_all(&self.body)?;
        out.flush()
    }
}

pub(crate) struct Message {
    pub(crate) kind: PacketType,
    pub(crate) id: u32,
    body: Vec<u8>,
    at: usize,
}

impl Message {
    pub(crate) fn receive(input: &mut impl Read) -> io::Result<Self> {
        Self::parse(read_frame(input)?)
    }

    pub(crate) fn parse(body: Vec<u8>) -> io::Result<Self> {
        let kind = body
            .first()
            .copied()
            .and_then(PacketType::from_byte)
            .ok_or_else(malformed)?;
        let mut message = Self {
            kind,
            id: 0,
            body,
            at: KIND_BYTES,
        };
        if kind.has_id() {
            message.id = message.u32()?;
        }
        Ok(message)
    }

    pub(crate) fn peek_id(body: &[u8]) -> Option<u32> {
        let id = body.get(KIND_BYTES..KIND_BYTES + ID_BYTES)?;
        id.try_into().ok().map(u32::from_be_bytes)
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
}

pub(crate) fn read_frame(input: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len = [0; LENGTH_BYTES];
    input.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len);
    if len == 0 || len > MAX_PACKET_BYTES {
        return Err(malformed());
    }
    let mut body = vec![0; usize::try_from(len).map_err(|_| malformed())?];
    input.read_exact(&mut body)?;
    Ok(body)
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

    fn reply_from(body: Vec<u8>) -> io::Result<Message> {
        let mut framed = u32::try_from(body.len())
            .map_err(|_| too_large())?
            .to_be_bytes()
            .to_vec();
        framed.extend(body);
        Message::receive(&mut framed.as_slice())
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
    fn truncated_and_oversized_packets_are_rejected() {
        assert!(reply_from(vec![PacketType::Status as u8, 0, 0]).is_err());
        let oversized = (MAX_PACKET_BYTES + 1).to_be_bytes();
        assert!(Message::receive(&mut oversized.as_slice()).is_err());
    }
}
