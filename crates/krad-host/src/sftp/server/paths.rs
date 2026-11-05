use std::fs;
use std::io;
use std::path::Path;

use super::super::wire::{Message, Packet};
use super::super::POSIX_RENAME_EXTENSION;
use super::{path, platform, reply};

const CURRENT_DIR: &str = ".";

pub(super) fn stat(message: &mut Message) -> io::Result<Packet> {
    let metadata = fs::metadata(path(message)?)?;
    Ok(reply::attrs(message.id, &platform::attrs(&metadata)))
}

pub(super) fn lstat(message: &mut Message) -> io::Result<Packet> {
    let metadata = fs::symlink_metadata(path(message)?)?;
    Ok(reply::attrs(message.id, &platform::attrs(&metadata)))
}

pub(super) fn setstat(message: &mut Message) -> io::Result<Packet> {
    let path = path(message)?;
    platform::set_path_attrs(&path, &message.attrs()?)?;
    reply::ok(message.id)
}

pub(super) fn remove(message: &mut Message) -> io::Result<Packet> {
    fs::remove_file(path(message)?)?;
    reply::ok(message.id)
}

pub(super) fn mkdir(message: &mut Message) -> io::Result<Packet> {
    let path = path(message)?;
    platform::create_dir(&path, message.attrs()?.mode())?;
    reply::ok(message.id)
}

pub(super) fn rmdir(message: &mut Message) -> io::Result<Packet> {
    fs::remove_dir(path(message)?)?;
    reply::ok(message.id)
}

pub(super) fn realpath(message: &mut Message) -> io::Result<Packet> {
    let path = path(message)?;
    let path = if path.as_os_str().is_empty() {
        Path::new(CURRENT_DIR)
    } else {
        &path
    };
    let real = fs::canonicalize(path)?;
    reply::name(message.id, platform::name(real.as_os_str()))
}

pub(super) fn rename(message: &mut Message) -> io::Result<Packet> {
    let from = path(message)?;
    let to = path(message)?;
    if fs::symlink_metadata(&to).is_ok() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    fs::rename(from, to)?;
    reply::ok(message.id)
}

pub(super) fn readlink(message: &mut Message) -> io::Result<Packet> {
    let target = fs::read_link(path(message)?)?;
    reply::name(message.id, platform::name(target.as_os_str()))
}

pub(super) fn symlink(message: &mut Message) -> io::Result<Packet> {
    let target = path(message)?;
    let link = path(message)?;
    platform::symlink(&target, &link)?;
    reply::ok(message.id)
}

pub(super) fn extended(message: &mut Message) -> io::Result<Packet> {
    if message.string()? != POSIX_RENAME_EXTENSION {
        return Err(io::ErrorKind::Unsupported.into());
    }
    let from = path(message)?;
    let to = path(message)?;
    fs::rename(from, to)?;
    reply::ok(message.id)
}
