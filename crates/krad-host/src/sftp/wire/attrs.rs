use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{Message, Packet};

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
    pub(crate) accessed_secs: Option<u32>,
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
        self.modified_secs.map(epoch_secs)
    }

    pub(crate) fn accessed(&self) -> Option<SystemTime> {
        self.accessed_secs.map(epoch_secs)
    }
}

fn epoch_secs(secs: u32) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(u64::from(secs))
}

impl Packet {
    pub(crate) fn empty_attrs(&mut self) -> &mut Self {
        self.u32(0)
    }

    pub(crate) fn attrs(&mut self, attrs: &Attrs) -> &mut Self {
        let times = attrs.accessed_secs.zip(attrs.modified_secs);
        let flags = [
            (attrs.size.is_some(), ATTR_SIZE),
            (attrs.owner.is_some(), ATTR_UIDGID),
            (attrs.permissions.is_some(), ATTR_PERMISSIONS),
            (times.is_some(), ATTR_ACMODTIME),
        ]
        .into_iter()
        .filter(|(present, _)| *present)
        .fold(0, |flags, (_, flag)| flags | flag);
        self.u32(flags);
        if let Some(size) = attrs.size {
            self.u64(size);
        }
        if let Some((uid, gid)) = attrs.owner {
            self.u32(uid).u32(gid);
        }
        if let Some(permissions) = attrs.permissions {
            self.u32(permissions);
        }
        if let Some((accessed, modified)) = times {
            self.u32(accessed).u32(modified);
        }
        self
    }

    pub(crate) fn mode_attrs(&mut self, mode: Option<u32>) -> &mut Self {
        match mode {
            Some(mode) => self.u32(ATTR_PERMISSIONS).u32(mode),
            None => self.empty_attrs(),
        }
    }
}

impl Message {
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
            attrs.accessed_secs = Some(self.u32()?);
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

#[cfg(test)]
mod tests {
    use super::super::PacketType;
    use super::*;

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
        let mut reply = Message::parse(body)?;
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
}
