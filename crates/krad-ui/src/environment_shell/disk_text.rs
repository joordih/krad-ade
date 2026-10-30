use std::path::Path;
use std::time::SystemTime;

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const TOO_LARGE_TEXT: &str = "This file is larger than 2 MB, too large to edit here.";
const NOT_TEXT_TEXT: &str = "This file is not UTF-8 text.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DiskChange {
    Unchanged,
    Reload,
    Conflict,
}

pub(super) fn disk_change(
    known: Option<SystemTime>,
    disk: Option<SystemTime>,
    dirty: bool,
) -> DiskChange {
    match disk {
        Some(disk) if Some(disk) != known && dirty => DiskChange::Conflict,
        Some(disk) if Some(disk) != known => DiskChange::Reload,
        _ => DiskChange::Unchanged,
    }
}

pub(super) struct ReadFile {
    pub(super) text: String,
    pub(super) modified: Option<SystemTime>,
}

#[derive(Debug)]
pub(super) enum ReadError {
    TooLarge,
    NotText,
    Io(String),
}

impl ReadError {
    pub(super) fn text(&self) -> &str {
        match self {
            Self::TooLarge => TOO_LARGE_TEXT,
            Self::NotText => NOT_TEXT_TEXT,
            Self::Io(reason) => reason,
        }
    }
}

pub(super) fn read_file(path: &Path) -> Result<ReadFile, ReadError> {
    let metadata = std::fs::metadata(path).map_err(|error| ReadError::Io(error.to_string()))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(ReadError::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(|error| ReadError::Io(error.to_string()))?;
    let text = String::from_utf8(bytes).map_err(|_| ReadError::NotText)?;
    Ok(ReadFile {
        text,
        modified: metadata.modified().ok(),
    })
}

pub(super) fn modified_at(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_disk_reloads_a_clean_tab_and_asks_about_a_dirty_one() {
        let before = SystemTime::UNIX_EPOCH;
        let after = before + Duration::from_secs(1);
        assert_eq!(
            disk_change(Some(before), Some(before), true),
            DiskChange::Unchanged
        );
        assert_eq!(
            disk_change(Some(before), Some(after), false),
            DiskChange::Reload
        );
        assert_eq!(
            disk_change(Some(before), Some(after), true),
            DiskChange::Conflict
        );
        assert_eq!(disk_change(Some(before), None, true), DiskChange::Unchanged);
    }
}
