mod dirs;
mod files;
mod handles;
mod paths;
#[cfg(unix)]
#[path = "server/unix.rs"]
mod platform;
#[cfg(not(unix))]
#[path = "server/portable.rs"]
mod platform;
mod reply;
#[cfg(all(test, unix))]
mod tests;
#[cfg(all(test, unix))]
mod through_client;

use std::fs::FileTimes;
use std::io::{self, BufWriter, Read, Write};
use std::path::PathBuf;

use super::wire::{read_frame, Attrs, Message, Packet, PacketType, StatusCode, PROTOCOL_VERSION};
use super::POSIX_RENAME_EXTENSION;
use handles::Handles;

const POSIX_RENAME_REVISION: &str = "1";
const NOT_AN_INIT: &str = "the SFTP client did not start with INIT";
const NO_REQUEST_ID: &str = "the SFTP request has no id";

pub(crate) fn serve(mut input: impl Read, output: impl Write) -> io::Result<()> {
    let mut output = BufWriter::new(output);
    let hello = Message::receive(&mut input)?;
    if hello.kind != PacketType::Init {
        return Err(io::Error::new(io::ErrorKind::InvalidData, NOT_AN_INIT));
    }
    let mut version = Packet::new(PacketType::Version);
    version
        .u32(PROTOCOL_VERSION)
        .str(POSIX_RENAME_EXTENSION)?
        .str(POSIX_RENAME_REVISION)?;
    version.send(&mut output)?;
    let mut server = Server::default();
    loop {
        let frame = match read_frame(&mut input) {
            Ok(frame) => frame,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error),
        };
        server.answer(frame)?.send(&mut output)?;
    }
}

#[derive(Default)]
struct Server {
    handles: Handles,
}

impl Server {
    fn answer(&mut self, frame: Vec<u8>) -> io::Result<Packet> {
        let id = Message::peek_id(&frame)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, NO_REQUEST_ID))?;
        let Ok(mut message) = Message::parse(frame) else {
            return reply::status(id, StatusCode::OpUnsupported);
        };
        match self.dispatch(&mut message) {
            Ok(packet) => Ok(packet),
            Err(error) => reply::refusal(id, &error),
        }
    }

    fn dispatch(&mut self, message: &mut Message) -> io::Result<Packet> {
        match message.kind {
            PacketType::Open => self.open(message),
            PacketType::Close => self.close(message),
            PacketType::Read => self.read(message),
            PacketType::Write => self.write(message),
            PacketType::Fstat => self.fstat(message),
            PacketType::Fsetstat => self.fsetstat(message),
            PacketType::Opendir => self.open_dir(message),
            PacketType::Readdir => self.read_dir(message),
            PacketType::Lstat => paths::lstat(message),
            PacketType::Stat => paths::stat(message),
            PacketType::Setstat => paths::setstat(message),
            PacketType::Remove => paths::remove(message),
            PacketType::Mkdir => paths::mkdir(message),
            PacketType::Rmdir => paths::rmdir(message),
            PacketType::Realpath => paths::realpath(message),
            PacketType::Rename => paths::rename(message),
            PacketType::Readlink => paths::readlink(message),
            PacketType::Symlink => paths::symlink(message),
            PacketType::Extended => paths::extended(message),
            PacketType::Init
            | PacketType::Version
            | PacketType::Status
            | PacketType::Handle
            | PacketType::Data
            | PacketType::Name
            | PacketType::Attrs => Err(io::ErrorKind::Unsupported.into()),
        }
    }
}

fn path(message: &mut Message) -> io::Result<PathBuf> {
    message.bytes().map(platform::path)
}

fn file_times(attrs: &Attrs) -> Option<FileTimes> {
    let (accessed, modified) = attrs.accessed().zip(attrs.modified())?;
    Some(
        FileTimes::new()
            .set_accessed(accessed)
            .set_modified(modified),
    )
}
