use std::fs;
use std::io;

use super::super::wire::{Message, Packet, StatusCode};
use super::handles::Opened;
use super::reply::{self, Named};
use super::{path, platform, Server};

const ENTRIES_PER_READ: usize = 100;

impl Server {
    pub(super) fn open_dir(&mut self, message: &mut Message) -> io::Result<Packet> {
        let dir = fs::read_dir(path(message)?)?;
        reply::handle(
            message.id,
            &self.handles.insert(Opened::Dir(Box::new(dir)))?,
        )
    }

    pub(super) fn read_dir(&mut self, message: &mut Message) -> io::Result<Packet> {
        let dir = self.handles.dir(&message.bytes()?)?;
        let mut entries = Vec::new();
        for entry in dir.by_ref() {
            let entry = entry?;
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            entries.push(Named {
                name: platform::name(&entry.file_name()),
                attrs: platform::attrs(&metadata),
            });
            if entries.len() == ENTRIES_PER_READ {
                break;
            }
        }
        if entries.is_empty() {
            return reply::status(message.id, StatusCode::Eof);
        }
        reply::names(message.id, &entries)
    }
}
