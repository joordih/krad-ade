use std::collections::VecDeque;
use std::io::Read;

use super::channel::Pending;
use super::wire::{PacketType, StatusCode};
use super::{status, Sftp, SftpError};

const CHUNK_BYTES: u32 = 32 * 1024;
const WINDOW: usize = 16;

enum Chunk {
    Data(Vec<u8>),
    End,
}

impl Sftp {
    fn request_read(&self, handle: &[u8], offset: u64, len: u32) -> Result<Pending<'_>, SftpError> {
        self.channel.send(PacketType::Read, |packet| {
            packet.bytes(handle)?.u64(offset).u32(len);
            Ok(())
        })
    }

    fn chunk(pending: Pending<'_>, path: &str) -> Result<Chunk, SftpError> {
        let mut reply = pending.wait()?;
        match reply.kind {
            PacketType::Data => {
                let data = reply.bytes()?;
                Ok(if data.is_empty() {
                    Chunk::End
                } else {
                    Chunk::Data(data)
                })
            }
            PacketType::Status => match status(&mut reply, path)? {
                StatusCode::Eof => Ok(Chunk::End),
                _ => Err(SftpError::Protocol),
            },
            _ => Err(SftpError::Protocol),
        }
    }

    pub(super) fn read_handle(
        &self,
        handle: &[u8],
        path: &str,
        from: u64,
        limit: u64,
    ) -> Result<Vec<u8>, SftpError> {
        let end = from.saturating_add(limit);
        let mut bytes = Vec::new();
        let mut next = from;
        let mut queue: VecDeque<(u64, u32, Pending<'_>)> = VecDeque::new();
        loop {
            while queue.len() < WINDOW && next < end {
                let len =
                    u32::try_from(end - next).map_or(CHUNK_BYTES, |left| left.min(CHUNK_BYTES));
                queue.push_back((next, len, self.request_read(handle, next, len)?));
                next += u64::from(len);
            }
            let Some((offset, len, pending)) = queue.pop_front() else {
                return Ok(bytes);
            };
            let Chunk::Data(data) = Self::chunk(pending, path)? else {
                return Ok(bytes);
            };
            let mut at = offset + byte_count(&data)?;
            bytes.extend(data);
            let wanted = offset + u64::from(len);
            while at < wanted {
                let rest = u32::try_from(wanted - at).map_err(|_| SftpError::Protocol)?;
                match Self::chunk(self.request_read(handle, at, rest)?, path)? {
                    Chunk::Data(data) => {
                        at += byte_count(&data)?;
                        bytes.extend(data);
                    }
                    Chunk::End => return Ok(bytes),
                }
            }
        }
    }

    pub(super) fn write_handle(
        &self,
        handle: &[u8],
        path: &str,
        bytes: &[u8],
    ) -> Result<(), SftpError> {
        let chunk_len = usize::try_from(CHUNK_BYTES).map_err(|_| SftpError::Protocol)?;
        let mut queue: VecDeque<Pending<'_>> = VecDeque::new();
        let mut offset: u64 = 0;
        for chunk in bytes.chunks(chunk_len) {
            if queue.len() == WINDOW {
                if let Some(pending) = queue.pop_front() {
                    Self::expect_ok(pending.wait()?, path)?;
                }
            }
            queue.push_back(self.channel.send(PacketType::Write, |packet| {
                packet.bytes(handle)?.u64(offset).bytes(chunk)?;
                Ok(())
            })?);
            offset += byte_count(chunk)?;
        }
        for pending in queue {
            Self::expect_ok(pending.wait()?, path)?;
        }
        Ok(())
    }
}

impl Sftp {
    /// # Errors
    pub fn write_from(
        &self,
        path: &str,
        source: &mut impl Read,
        mode: Option<u32>,
    ) -> Result<u64, SftpError> {
        let handle = self.open(path, super::wire::OpenFlags::WRITE_NEW, mode)?;
        let written = self.stream_into(&handle, path, source);
        self.close(&handle, path)?;
        written
    }

    pub(super) fn stream_into(
        &self,
        handle: &[u8],
        path: &str,
        source: &mut impl Read,
    ) -> Result<u64, SftpError> {
        let chunk_len = usize::try_from(CHUNK_BYTES).map_err(|_| SftpError::Protocol)?;
        let mut buffer = vec![0; chunk_len];
        let mut queue: VecDeque<Pending<'_>> = VecDeque::new();
        let mut offset: u64 = 0;
        loop {
            let read = source.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            if queue.len() == WINDOW {
                if let Some(pending) = queue.pop_front() {
                    Self::expect_ok(pending.wait()?, path)?;
                }
            }
            let chunk = &buffer[..read];
            queue.push_back(self.channel.send(PacketType::Write, |packet| {
                packet.bytes(handle)?.u64(offset).bytes(chunk)?;
                Ok(())
            })?);
            offset += byte_count(chunk)?;
        }
        for pending in queue {
            Self::expect_ok(pending.wait()?, path)?;
        }
        Ok(offset)
    }
}

fn byte_count(bytes: &[u8]) -> Result<u64, SftpError> {
    u64::try_from(bytes.len()).map_err(|_| SftpError::Protocol)
}
