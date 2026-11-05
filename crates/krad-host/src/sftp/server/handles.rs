use std::collections::HashMap;
use std::fs::{File, ReadDir};
use std::io;

const MAX_OPEN_HANDLES: usize = 256;
const HANDLE_BYTES: usize = 4;
const TOO_MANY_HANDLES: &str = "too many open SFTP handles";
const UNKNOWN_HANDLE: &str = "no such SFTP handle";

pub(super) enum Opened {
    File(File),
    Dir(Box<ReadDir>),
}

#[derive(Default)]
pub(super) struct Handles {
    open: HashMap<u32, Opened>,
    next: u32,
}

impl Handles {
    pub(super) fn insert(&mut self, opened: Opened) -> io::Result<[u8; HANDLE_BYTES]> {
        if self.open.len() >= MAX_OPEN_HANDLES {
            return Err(io::Error::other(TOO_MANY_HANDLES));
        }
        while self.open.contains_key(&self.next) {
            self.next = self.next.wrapping_add(1);
        }
        let key = self.next;
        self.next = self.next.wrapping_add(1);
        self.open.insert(key, opened);
        Ok(key.to_be_bytes())
    }

    pub(super) fn file(&mut self, handle: &[u8]) -> io::Result<&mut File> {
        match self.open.get_mut(&key(handle)?) {
            Some(Opened::File(file)) => Ok(file),
            _ => Err(unknown()),
        }
    }

    pub(super) fn dir(&mut self, handle: &[u8]) -> io::Result<&mut ReadDir> {
        match self.open.get_mut(&key(handle)?) {
            Some(Opened::Dir(dir)) => Ok(dir),
            _ => Err(unknown()),
        }
    }

    pub(super) fn close(&mut self, handle: &[u8]) -> io::Result<()> {
        self.open
            .remove(&key(handle)?)
            .map(drop)
            .ok_or_else(unknown)
    }
}

fn key(handle: &[u8]) -> io::Result<u32> {
    <[u8; HANDLE_BYTES]>::try_from(handle)
        .map(u32::from_be_bytes)
        .map_err(|_| unknown())
}

fn unknown() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, UNKNOWN_HANDLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_bounded_and_unknown_ones_are_refused() -> io::Result<()> {
        let dir = std::env::temp_dir();
        let mut handles = Handles::default();
        let first = handles.insert(Opened::Dir(Box::new(std::fs::read_dir(&dir)?)))?;
        for _ in 1..MAX_OPEN_HANDLES {
            handles.insert(Opened::Dir(Box::new(std::fs::read_dir(&dir)?)))?;
        }
        assert!(handles
            .insert(Opened::Dir(Box::new(std::fs::read_dir(&dir)?)))
            .is_err());
        assert!(handles.file(&first).is_err());
        assert!(handles.dir(&first).is_ok());
        assert!(handles.dir(&[1, 2, 3]).is_err());
        handles.close(&first)?;
        assert!(handles.close(&first).is_err());
        assert!(handles
            .insert(Opened::Dir(Box::new(std::fs::read_dir(&dir)?)))
            .is_ok());
        Ok(())
    }
}
