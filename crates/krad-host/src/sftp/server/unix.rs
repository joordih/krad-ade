use std::ffi::{OsStr, OsString};
use std::fs::{self, DirBuilder, File, Metadata, OpenOptions, Permissions};
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{
    chown, fchown, symlink as make_symlink, DirBuilderExt, MetadataExt, OpenOptionsExt,
    PermissionsExt,
};
use std::path::{Path, PathBuf};

use super::super::wire::Attrs;
use super::file_times;

const DEFAULT_FILE_MODE: u32 = 0o666;
const DEFAULT_DIR_MODE: u32 = 0o777;

pub(super) fn path(bytes: Vec<u8>) -> PathBuf {
    PathBuf::from(OsString::from_vec(bytes))
}

pub(super) fn name(name: &OsStr) -> Vec<u8> {
    name.as_bytes().to_vec()
}

pub(super) fn attrs(metadata: &Metadata) -> Attrs {
    Attrs {
        size: Some(metadata.size()),
        owner: Some((metadata.uid(), metadata.gid())),
        permissions: Some(metadata.mode()),
        accessed_secs: Some(epoch_secs(metadata.atime())),
        modified_secs: Some(epoch_secs(metadata.mtime())),
    }
}

fn epoch_secs(secs: i64) -> u32 {
    u32::try_from(secs.clamp(0, i64::from(u32::MAX))).unwrap_or_default()
}

pub(super) fn create_with_mode(options: &mut OpenOptions, mode: Option<u32>) {
    options.mode(mode.unwrap_or(DEFAULT_FILE_MODE));
}

pub(super) fn create_dir(path: &Path, mode: Option<u32>) -> io::Result<()> {
    DirBuilder::new()
        .mode(mode.unwrap_or(DEFAULT_DIR_MODE))
        .create(path)
}

pub(super) fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    make_symlink(target, link)
}

pub(super) fn set_path_attrs(path: &Path, attrs: &Attrs) -> io::Result<()> {
    if let Some(size) = attrs.size {
        OpenOptions::new().write(true).open(path)?.set_len(size)?;
    }
    if let Some(mode) = attrs.mode() {
        fs::set_permissions(path, Permissions::from_mode(mode))?;
    }
    if let Some(times) = file_times(attrs) {
        File::open(path)?.set_times(times)?;
    }
    if let Some((uid, gid)) = attrs.owner {
        chown(path, Some(uid), Some(gid))?;
    }
    Ok(())
}

pub(super) fn set_file_attrs(file: &File, attrs: &Attrs) -> io::Result<()> {
    if let Some(size) = attrs.size {
        file.set_len(size)?;
    }
    if let Some(mode) = attrs.mode() {
        file.set_permissions(Permissions::from_mode(mode))?;
    }
    if let Some(times) = file_times(attrs) {
        file.set_times(times)?;
    }
    if let Some((uid, gid)) = attrs.owner {
        fchown(file, Some(uid), Some(gid))?;
    }
    Ok(())
}
