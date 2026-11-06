use std::io;

use super::super::wire::{Attrs, Packet, PacketType, StatusCode};

const LANGUAGE_TAG: &str = "";
const OK_TEXT: &str = "Success";
const EOF_TEXT: &str = "End of file";
const NO_SUCH_FILE_TEXT: &str = "No such file";
const PERMISSION_DENIED_TEXT: &str = "Permission denied";
const FAILURE_TEXT: &str = "Failure";
const BAD_MESSAGE_TEXT: &str = "Bad message";
const OP_UNSUPPORTED_TEXT: &str = "Operation unsupported";

pub(super) fn status(id: u32, code: StatusCode) -> io::Result<Packet> {
    let mut packet = Packet::request(PacketType::Status, id);
    packet.u32(code.into()).str(text(code))?.str(LANGUAGE_TAG)?;
    Ok(packet)
}

pub(super) fn ok(id: u32) -> io::Result<Packet> {
    status(id, StatusCode::Ok)
}

pub(super) fn refusal(id: u32, error: &io::Error) -> io::Result<Packet> {
    tracing::trace!(kind = %error.kind(), "the built-in SFTP server refused a request");
    status(id, code_for(error))
}

pub(super) fn handle(id: u32, handle: &[u8]) -> io::Result<Packet> {
    let mut packet = Packet::request(PacketType::Handle, id);
    packet.bytes(handle)?;
    Ok(packet)
}

pub(super) fn data(id: u32, data: &[u8]) -> io::Result<Packet> {
    let mut packet = Packet::request(PacketType::Data, id);
    packet.bytes(data)?;
    Ok(packet)
}

pub(super) fn attrs(id: u32, attrs: &Attrs) -> Packet {
    let mut packet = Packet::request(PacketType::Attrs, id);
    packet.attrs(attrs);
    packet
}

pub(super) struct Named {
    pub(super) name: Vec<u8>,
    pub(super) attrs: Attrs,
}

pub(super) fn names(id: u32, entries: &[Named]) -> io::Result<Packet> {
    let mut packet = Packet::request(PacketType::Name, id);
    let count = u32::try_from(entries.len()).map_err(|_| io::ErrorKind::InvalidInput)?;
    packet.u32(count);
    for entry in entries {
        packet
            .bytes(&entry.name)?
            .bytes(&entry.name)?
            .attrs(&entry.attrs);
    }
    Ok(packet)
}

pub(super) fn name(id: u32, name: Vec<u8>) -> io::Result<Packet> {
    names(
        id,
        &[Named {
            name,
            attrs: Attrs::default(),
        }],
    )
}

fn code_for(error: &io::Error) -> StatusCode {
    match error.kind() {
        io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        io::ErrorKind::Unsupported => StatusCode::OpUnsupported,
        io::ErrorKind::InvalidData => StatusCode::BadMessage,
        _ => StatusCode::Failure,
    }
}

fn text(code: StatusCode) -> &'static str {
    match code {
        StatusCode::Ok => OK_TEXT,
        StatusCode::Eof => EOF_TEXT,
        StatusCode::NoSuchFile => NO_SUCH_FILE_TEXT,
        StatusCode::PermissionDenied => PERMISSION_DENIED_TEXT,
        StatusCode::BadMessage => BAD_MESSAGE_TEXT,
        StatusCode::OpUnsupported => OP_UNSUPPORTED_TEXT,
        StatusCode::Failure | StatusCode::Other(_) => FAILURE_TEXT,
    }
}
