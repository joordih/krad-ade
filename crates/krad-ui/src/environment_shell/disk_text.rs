use krad_client::ClientError;
use krad_protocol::{Answer, FileAsk, OverLimit, Unreadable};
use std::path::PathBuf;

use super::file_asks::Statted;
use std::time::SystemTime;

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const TOO_LARGE_TEXT: &str = "This file is larger than 2 MB, too large to edit here.";
const NOT_TEXT_TEXT: &str = "This file is not UTF-8 text.";
const UNEXPECTED_TEXT: &str = "The runtime answered something else.";

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

pub(super) fn read_ask(path: PathBuf) -> FileAsk {
    FileAsk::Read {
        path,
        from: 0,
        limit: MAX_FILE_BYTES,
        over: OverLimit::Refuse,
    }
}

pub(super) fn stat_ask(path: PathBuf) -> FileAsk {
    FileAsk::Stat { path }
}

pub(super) fn read_of(answer: Result<Answer, ClientError>) -> Result<ReadFile, ReadError> {
    match answer {
        Ok(Answer::FileText { text, modified, .. }) => Ok(ReadFile { text, modified }),
        Ok(Answer::Unreadable {
            unreadable: Unreadable::TooLarge { .. },
        }) => Err(ReadError::TooLarge),
        Ok(Answer::Unreadable {
            unreadable: Unreadable::NotText,
        }) => Err(ReadError::NotText),
        Ok(_) => Err(ReadError::Io(UNEXPECTED_TEXT.to_owned())),
        Err(error) => Err(ReadError::Io(error.to_string())),
    }
}

pub(super) fn modified_of(stat: Result<Statted, ClientError>) -> Option<SystemTime> {
    stat.ok().and_then(|Statted(stat)| stat?.modified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_read_answer_becomes_the_tab_s_text_or_the_reason_it_cannot_open() {
        let text = read_of(Ok(Answer::FileText {
            text: "fn main() {}".to_owned(),
            len: 12,
            truncated: false,
            modified: None,
        }));
        assert!(matches!(text, Ok(ReadFile { ref text, .. }) if text == "fn main() {}"));
        let large = read_of(Ok(Answer::Unreadable {
            unreadable: Unreadable::TooLarge { len: 9, limit: 4 },
        }));
        assert!(matches!(large, Err(ReadError::TooLarge)));
        let binary = read_of(Ok(Answer::Unreadable {
            unreadable: Unreadable::NotText,
        }));
        assert!(matches!(binary, Err(ReadError::NotText)));
        let gone = read_of(Err(ClientError::Stopped));
        assert!(matches!(gone, Err(ReadError::Io(_))));
    }

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
