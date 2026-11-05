use std::ffi::OsStr;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::super::wire::Attrs;
use super::file_times;

const FILE_TYPE_DIRECTORY: u32 = 0o040_000;
const FILE_TYPE_REGULAR: u32 = 0o100_000;
const FILE_TYPE_SYMLINK: u32 = 0o120_000;
const WRITABLE_MODE: u32 = 0o666;
const READ_ONLY_MODE: u32 = 0o444;
const OWNER_WRITE: u32 = 0o200;

pub(super) fn path(bytes: Vec<u8>) -> PathBuf {
    PathBuf::from(
        String::from_utf8(bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned()),
    )
}

pub(super) fn name(name: &OsStr) -> Vec<u8> {
    name.to_string_lossy().into_owned().into_bytes()
}

pub(super) fn attrs(metadata: &Metadata) -> Attrs {
    let kind = if metadata.is_symlink() {
        FILE_TYPE_SYMLINK
    } else if metadata.is_dir() {
        FILE_TYPE_DIRECTORY
    } else {
        FILE_TYPE_REGULAR
    };
    let mode = if metadata.permissions().readonly() {
        READ_ONLY_MODE
    } else {
        WRITABLE_MODE
    };
    Attrs {
        size: Some(metadata.len()),
        owner: None,
        permissions: Some(kind | mode),
        accessed_secs: metadata.accessed().ok().map(epoch_secs),
        modified_secs: metadata.modified().ok().map(epoch_secs),
    }
}

fn epoch_secs(time: SystemTime) -> u32 {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| u32::try_from(since.as_secs()).ok())
        .unwrap_or_default()
}

pub(super) fn create_with_mode(_options: &mut OpenOptions, _mode: Option<u32>) {}

pub(super) fn create_dir(path: &Path, _mode: Option<u32>) -> io::Result<()> {
    fs::create_dir(path)
}

pub(super) fn symlink(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

pub(super) fn set_path_attrs(path: &Path, attrs: &Attrs) -> io::Result<()> {
    if attrs.owner.is_some() {
        return Err(io::ErrorKind::Unsupported.into());
    }
    if let Some(size) = attrs.size {
        OpenOptions::new().write(true).open(path)?.set_len(size)?;
    }
    if let Some(mode) = attrs.mode() {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_readonly(mode & OWNER_WRITE == 0);
        fs::set_permissions(path, permissions)?;
    }
    if let Some(times) = file_times(attrs) {
        OpenOptions::new()
            .write(true)
            .open(path)?
            .set_times(times)?;
    }
    Ok(())
}

pub(super) fn set_file_attrs(file: &File, attrs: &Attrs) -> io::Result<()> {
    if attrs.owner.is_some() {
        return Err(io::ErrorKind::Unsupported.into());
    }
    if let Some(size) = attrs.size {
        file.set_len(size)?;
    }
    if let Some(mode) = attrs.mode() {
        let mut permissions = file.metadata()?.permissions();
        permissions.set_readonly(mode & OWNER_WRITE == 0);
        file.set_permissions(permissions)?;
    }
    if let Some(times) = file_times(attrs) {
        file.set_times(times)?;
    }
    Ok(())
}
