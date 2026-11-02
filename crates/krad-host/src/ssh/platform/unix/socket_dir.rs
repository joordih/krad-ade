use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

const SOCKET_PATH_MAX_BYTES: usize = 100;
const SOCKET_NAME_BYTES: usize = 40;
const SOCKET_TEMP_SUFFIX_BYTES: usize = 17;
const PATH_SEPARATOR_BYTES: usize = 1;
const RUNTIME_DIR_VAR: &str = "XDG_RUNTIME_DIR";
const TEMP_DIR_VAR: &str = "TMPDIR";
const SHARED_TEMP_ROOT: &str = "/tmp";
const RUNTIME_SUBDIR: &str = "krad-ade";
const CONFIG_SUBDIR: &str = "ssh";
const TEMP_PREFIX: &str = "krad-ade-";
const PRIVATE_DIR_MODE: u32 = 0o700;
const FOREIGN_OWNER: &str = "the SSH control folder belongs to another user";
const NOT_A_FOLDER: &str = "the SSH control folder is not a folder";
const NO_SHORT_FOLDER: &str = "no folder is short enough for SSH control sockets";

pub(super) fn prepare(config_dir: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(config_dir)?;
    let uid = fs::metadata(config_dir)?.uid();
    let candidates = candidates(
        config_dir,
        uid,
        std::env::var_os(RUNTIME_DIR_VAR),
        std::env::var_os(TEMP_DIR_VAR),
    );
    let dir = candidates
        .into_iter()
        .find(|dir| fits(dir))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, NO_SHORT_FOLDER))?;
    create_private_dir(&dir, uid)?;
    Ok(dir)
}

fn candidates(
    config_dir: &Path,
    uid: u32,
    runtime: Option<OsString>,
    temp: Option<OsString>,
) -> Vec<PathBuf> {
    let per_user = format!("{TEMP_PREFIX}{uid}");
    let mut dirs = Vec::new();
    if let Some(runtime) = runtime.filter(|dir| !dir.is_empty()) {
        dirs.push(PathBuf::from(runtime).join(RUNTIME_SUBDIR));
    }
    dirs.push(config_dir.join(CONFIG_SUBDIR));
    if let Some(temp) = temp.filter(|dir| !dir.is_empty()) {
        dirs.push(PathBuf::from(temp).join(&per_user));
    }
    dirs.push(Path::new(SHARED_TEMP_ROOT).join(per_user));
    dirs
}

fn fits(dir: &Path) -> bool {
    dir.as_os_str().len() + PATH_SEPARATOR_BYTES + SOCKET_NAME_BYTES + SOCKET_TEMP_SUFFIX_BYTES
        <= SOCKET_PATH_MAX_BYTES
}

fn create_private_dir(dir: &Path, uid: u32) -> io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(PRIVATE_DIR_MODE)
        .create(dir)?;
    let metadata = fs::symlink_metadata(dir)?;
    if !metadata.is_dir() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, NOT_A_FOLDER));
    }
    if metadata.uid() != uid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            FOREIGN_OWNER,
        ));
    }
    fs::set_permissions(dir, fs::Permissions::from_mode(PRIVATE_DIR_MODE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runtime_folder_comes_first_and_the_shared_temp_last() {
        let dirs = candidates(
            Path::new("/home/me/.config/krad-ade"),
            1000,
            Some(OsString::from("/run/user/1000")),
            Some(OsString::from("/var/folders/ab/T")),
        );
        assert_eq!(
            dirs,
            [
                PathBuf::from("/run/user/1000/krad-ade"),
                PathBuf::from("/home/me/.config/krad-ade/ssh"),
                PathBuf::from("/var/folders/ab/T/krad-ade-1000"),
                PathBuf::from("/tmp/krad-ade-1000"),
            ]
        );
    }

    #[test]
    fn long_folders_are_skipped_for_shorter_ones() {
        let long = Path::new("/home/me").join("x".repeat(80));
        let dirs = candidates(&long, 501, None, None);
        let chosen = dirs.into_iter().find(|dir| fits(dir));
        assert_eq!(chosen, Some(PathBuf::from("/tmp/krad-ade-501")));
    }

    #[test]
    fn a_prepared_folder_is_private_to_its_owner() -> io::Result<()> {
        let base = std::env::temp_dir().join(format!("krad-control-{}", std::process::id()));
        let dir = prepare(&base)?;
        assert_eq!(
            fs::metadata(&dir)?.permissions().mode() & 0o777,
            PRIVATE_DIR_MODE
        );
        let uid = fs::metadata(&base)?.uid();
        assert!(create_private_dir(&dir, uid.wrapping_add(1)).is_err());
        if !dir.starts_with(&base) {
            fs::remove_dir(&dir).ok();
        }
        fs::remove_dir_all(base)
    }
}
