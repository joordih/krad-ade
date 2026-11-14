use krad_domain::{EnvironmentName, FileMatches, ProjectQuery, ReplaceUndo, UploadExisting};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::ids::RequestId;

pub const MAX_READ_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_WRITE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_ATTACHMENT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_FOUND_LINES: usize = 5_000;
pub const MAX_TEXTS_READ: usize = 256;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct FileCommand {
    pub environment: EnvironmentName,
    pub ask: FileAsk,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverLimit {
    #[default]
    Truncate,
    Refuse,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum FileAsk {
    Read {
        path: PathBuf,
        #[serde(default)]
        from: u64,
        limit: u64,
        #[serde(default)]
        over: OverLimit,
    },
    Stat {
        path: PathBuf,
    },
    Write {
        path: PathBuf,
        text: String,
    },
    List {
        dir: PathBuf,
    },
    CreateFile {
        dir: PathBuf,
        name: String,
    },
    CreateDir {
        dir: PathBuf,
        name: String,
    },
    Rename {
        path: PathBuf,
        name: String,
    },
    Move {
        path: PathBuf,
        into: PathBuf,
    },
    Relocate {
        path: PathBuf,
        target: PathBuf,
    },
    Delete {
        path: PathBuf,
    },
    Taken {
        dir: PathBuf,
        names: Vec<String>,
    },
    Present {
        paths: Vec<PathBuf>,
    },
    Contained {
        root: PathBuf,
        path: PathBuf,
    },
    ReadTexts {
        paths: Vec<PathBuf>,
        limit: u64,
    },
    Index {
        root: PathBuf,
    },
    CreateFiles {
        folders: Vec<PathBuf>,
        files: Vec<NewFile>,
    },
    Upload {
        dir: PathBuf,
        sources: Vec<PathBuf>,
        #[serde(default)]
        existing: UploadExisting,
    },
    CancelUpload {
        upload: RequestId,
    },
    Search {
        root: PathBuf,
        query: ProjectQuery,
        limit: usize,
    },
    Replace {
        files: Vec<FileMatches>,
        replacement: String,
    },
    UndoReplace {
        undos: Vec<ReplaceUndo>,
    },
    SearchText {
        dir: PathBuf,
        query: String,
        limit: usize,
    },
    Vault {
        root: PathBuf,
    },
    CreateNote {
        root: PathBuf,
        folder: String,
        title: String,
    },
    CreateVaultFolder {
        root: PathBuf,
        folder: String,
        name: String,
    },
    WriteNote {
        root: PathBuf,
        path: String,
        text: String,
    },
    SaveAttachment {
        root: PathBuf,
        stem: String,
        extension: String,
        bytes: Vec<u8>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum Unreadable {
    TooLarge { len: u64, limit: u64 },
    NotText,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TextRead {
    pub path: PathBuf,
    pub text: String,
    pub truncated: bool,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct NewFile {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VaultNoteText {
    pub path: String,
    pub text: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct UploadShown {
    pub upload: RequestId,
    pub sent: u64,
    pub total: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::answers::Answer;
    use crate::command::Command;
    use crate::event::EventKind;

    #[test]
    fn a_file_ask_names_its_environment_and_verb_on_the_wire() -> serde_json::Result<()> {
        let command = Command::File(FileCommand {
            environment: EnvironmentName::new("main".to_owned()),
            ask: FileAsk::Read {
                path: PathBuf::from("/work/app/src/main.rs"),
                from: 0,
                limit: MAX_READ_BYTES,
                over: OverLimit::Refuse,
            },
        });
        let json = serde_json::to_value(&command)?;
        assert_eq!(json["area"], "file");
        assert_eq!(json["ask"]["verb"], "read");
        assert_eq!(json["ask"]["over"], "refuse");
        assert_eq!(serde_json::from_value::<Command>(json)?, command);
        let older: FileAsk = serde_json::from_value(
            serde_json::json!({ "verb": "read", "path": "/a", "limit": 10 }),
        )?;
        assert!(matches!(
            older,
            FileAsk::Read {
                from: 0,
                over: OverLimit::Truncate,
                ..
            }
        ));
        let newer: FileAsk = serde_json::from_value(serde_json::json!({ "verb": "chmod" }))?;
        assert_eq!(newer, FileAsk::Unknown);
        Ok(())
    }

    #[test]
    fn an_unreadable_file_and_an_upload_say_why_and_how_far_on_the_wire() -> serde_json::Result<()>
    {
        let answer = Answer::Unreadable {
            unreadable: Unreadable::TooLarge { len: 9, limit: 4 },
        };
        let json = serde_json::to_value(&answer)?;
        assert_eq!(json["kind"], "unreadable");
        assert_eq!(json["unreadable"]["reason"], "too_large");
        assert_eq!(serde_json::from_value::<Answer>(json)?, answer);
        let progress = EventKind::UploadProgress(UploadShown {
            upload: RequestId(3),
            sent: 5,
            total: 10,
        });
        let json = serde_json::to_value(&progress)?;
        assert_eq!(json["type"], "upload_progress");
        assert_eq!(serde_json::from_value::<EventKind>(json)?, progress);
        Ok(())
    }
}
