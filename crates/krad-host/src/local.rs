use std::ffi::OsString;
use std::process::Command;

const SHELL_VAR: &str = "SHELL";
#[cfg(not(windows))]
const FALLBACK_SHELL: &str = "sh";
#[cfg(windows)]
const FALLBACK_SHELL: &str = "powershell.exe";

pub(crate) fn default_shell() -> OsString {
    std::env::var_os(SHELL_VAR)
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| OsString::from(FALLBACK_SHELL))
}

#[cfg(windows)]
pub fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub const fn hide_console_window(_command: &mut Command) {}

#[cfg(unix)]
pub(crate) fn write_with_mode(
    path: &std::path::Path,
    bytes: &[u8],
    mode: u32,
) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(path)?;
    file.write_all(bytes)?;
    set_permissions(path, mode)
}

#[cfg(not(unix))]
pub(crate) fn write_with_mode(
    path: &std::path::Path,
    bytes: &[u8],
    _mode: u32,
) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

#[cfg(unix)]
pub(crate) fn create_dir_with_mode(path: &std::path::Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    std::fs::DirBuilder::new().mode(mode).create(path)
}

#[cfg(not(unix))]
pub(crate) fn create_dir_with_mode(path: &std::path::Path, _mode: u32) -> std::io::Result<()> {
    std::fs::create_dir(path)
}

#[cfg(unix)]
pub(crate) fn set_permissions(path: &std::path::Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
pub(crate) fn set_permissions(path: &std::path::Path, mode: u32) -> std::io::Result<()> {
    const ANY_WRITE: u32 = 0o222;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_readonly(mode & ANY_WRITE == 0);
    std::fs::set_permissions(path, permissions)
}

#[cfg(unix)]
pub(crate) fn executable_mode(metadata: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;

    const PERMISSION_BITS: u32 = 0o777;
    const ANY_EXECUTE: u32 = 0o111;
    let mode = metadata.permissions().mode() & PERMISSION_BITS;
    (mode & ANY_EXECUTE != 0).then_some(mode)
}

#[cfg(not(unix))]
pub(crate) fn executable_mode(_metadata: &std::fs::Metadata) -> Option<u32> {
    None
}

#[cfg(unix)]
pub(crate) fn symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
pub(crate) fn symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    }
}
