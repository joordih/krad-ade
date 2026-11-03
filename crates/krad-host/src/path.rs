use std::path::Path;

#[cfg(windows)]
const CLIENT_SEPARATOR: char = '\\';
#[cfg(windows)]
const REMOTE_SEPARATOR: &str = "/";

#[cfg(not(windows))]
#[must_use]
pub fn remote_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(windows)]
#[must_use]
pub fn remote_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace(CLIENT_SEPARATOR, REMOTE_SEPARATOR)
}
