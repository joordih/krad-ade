use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};

use super::super::wire::{Message, OpenFlags, Packet, StatusCode};
use super::handles::Opened;
use super::{path, platform, reply, Server};

const MAX_READ_BYTES: u32 = 64 * 1024;

impl Server {
    pub(super) fn open(&mut self, message: &mut Message) -> io::Result<Packet> {
        let path = path(message)?;
        let flags = OpenFlags(message.u32()?);
        let mode = message.attrs()?.mode();
        let file = open_options(flags, mode).open(path)?;
        reply::handle(message.id, &self.handles.insert(Opened::File(file))?)
    }

    pub(super) fn close(&mut self, message: &mut Message) -> io::Result<Packet> {
        self.handles.close(&message.bytes()?)?;
        reply::ok(message.id)
    }

    pub(super) fn read(&mut self, message: &mut Message) -> io::Result<Packet> {
        let handle = message.bytes()?;
        let offset = message.u64()?;
        let len = message.u32()?.min(MAX_READ_BYTES);
        let file = self.handles.file(&handle)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut data = Vec::new();
        Read::by_ref(file)
            .take(u64::from(len))
            .read_to_end(&mut data)?;
        if data.is_empty() {
            return reply::status(message.id, StatusCode::Eof);
        }
        reply::data(message.id, &data)
    }

    pub(super) fn write(&mut self, message: &mut Message) -> io::Result<Packet> {
        let handle = message.bytes()?;
        let offset = message.u64()?;
        let data = message.bytes()?;
        let file = self.handles.file(&handle)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&data)?;
        reply::ok(message.id)
    }

    pub(super) fn fstat(&mut self, message: &mut Message) -> io::Result<Packet> {
        let file: &File = self.handles.file(&message.bytes()?)?;
        Ok(reply::attrs(
            message.id,
            &platform::attrs(&file.metadata()?),
        ))
    }

    pub(super) fn fsetstat(&mut self, message: &mut Message) -> io::Result<Packet> {
        let handle = message.bytes()?;
        let attrs = message.attrs()?;
        platform::set_file_attrs(self.handles.file(&handle)?, &attrs)?;
        reply::ok(message.id)
    }
}

fn open_options(flags: OpenFlags, mode: Option<u32>) -> OpenOptions {
    let create = flags.contains(OpenFlags::CREATE);
    let exclusive = create && flags.contains(OpenFlags::EXCLUSIVE);
    let mut options = OpenOptions::new();
    options
        .read(flags.contains(OpenFlags::READ))
        .write(flags.contains(OpenFlags::WRITE))
        .append(flags.contains(OpenFlags::APPEND))
        .truncate(flags.contains(OpenFlags::TRUNCATE))
        .create(create && !exclusive)
        .create_new(exclusive);
    platform::create_with_mode(&mut options, mode);
    options
}
