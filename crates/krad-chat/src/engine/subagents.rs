use krad_domain::{ChatEntry, ChatEvent, ResumeId};
use std::fs;
use std::io::{self, Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};

use super::history::transcript;
use super::session::Record;
use super::tools::Workspace;
use crate::history::TranscriptChunk;
use crate::subagents::{
    SubagentFiles, SubagentMeta, META_EXTENSION, RECORD_EXTENSION, SUBAGENTS_DIR,
};

const LINE_END: u8 = b'\n';
const UNSAFE_REPLACEMENT: char = '_';
const SAFE_PUNCTUATION: [char; 2] = ['-', '_'];

pub(crate) fn subagents_dir(sessions: &Path, resume: &ResumeId) -> PathBuf {
    sessions.join(resume.as_str()).join(SUBAGENTS_DIR)
}

fn files_of(record: PathBuf) -> SubagentFiles {
    SubagentFiles {
        meta: record.with_extension(META_EXTENSION),
        record,
    }
}

pub(crate) fn subagent_files(dir: &Path, call: &str) -> SubagentFiles {
    let stem: String = call
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() || SAFE_PUNCTUATION.contains(&char) {
                char
            } else {
                UNSAFE_REPLACEMENT
            }
        })
        .collect();
    files_of(dir.join(stem).with_extension(RECORD_EXTENSION))
}

pub(crate) fn write_meta(files: &SubagentFiles, meta: &SubagentMeta) -> io::Result<()> {
    let text = serde_json::to_string(meta).map_err(io::Error::other)?;
    fs::write(&files.meta, text)
}

#[must_use]
pub fn engine_subagents(sessions: &Path, resume: &ResumeId) -> Vec<SubagentFiles> {
    let Ok(entries) = fs::read_dir(subagents_dir(sessions, resume)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == RECORD_EXTENSION)
        })
        .map(files_of)
        .collect()
}

/// # Errors
///
/// Returns the I/O error when the record cannot be opened or read.
pub fn read_engine_subagent(record: &Path, cwd: &Path, from: u64) -> io::Result<TranscriptChunk> {
    let mut reader = fs::File::open(record)?;
    reader.seek(SeekFrom::Start(from))?;
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    let Some(end) = bytes.iter().rposition(|byte| *byte == LINE_END) else {
        return Ok(TranscriptChunk {
            entries: Vec::new(),
            next: from,
        });
    };
    let records = String::from_utf8_lossy(&bytes[..end])
        .lines()
        .filter_map(|line| serde_json::from_str::<Record>(line).ok())
        .collect();
    let workspace = Workspace {
        cwd: cwd.to_path_buf(),
        env: Vec::new(),
    };
    Ok(TranscriptChunk {
        entries: transcript(records, &workspace)
            .into_iter()
            .filter(|entry| !matches!(entry, ChatEntry::Event(ChatEvent::Prompted(_))))
            .collect(),
        next: from + u64::try_from(end + 1).unwrap_or(u64::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::session::SessionFile;
    use crate::engine::wire::Message;
    use crate::live::new_session_id;
    use krad_domain::ToolUseId;

    #[test]
    fn a_subagent_is_listed_with_its_note_and_read_from_where_the_card_left_off() -> io::Result<()>
    {
        let sessions = std::env::temp_dir()
            .join("krad-engine-subagents")
            .join(new_session_id().as_str());
        let resume = new_session_id();
        let dir = subagents_dir(&sessions, &resume);
        let files = subagent_files(&dir, "call/1");
        assert_eq!(files.record, dir.join("call_1.jsonl"));
        let mut session = SessionFile::create(&files.record, new_session_id())?;
        write_meta(
            &files,
            &SubagentMeta {
                tool_use_id: ToolUseId::from("call/1"),
                agent_type: None,
                description: Some("Map the UI".to_owned()),
                finished: false,
            },
        )?;
        session.append(&Record::Message(Message::user(
            "Find every view".to_owned(),
        )))?;
        let listed = engine_subagents(&sessions, &resume);
        assert_eq!(listed, std::slice::from_ref(&files));
        let first = read_engine_subagent(&files.record, &sessions, 0)?;
        assert!(
            matches!(first.entries.as_slice(), [ChatEntry::User { text, .. }] if text == "Find every view")
        );
        let again = read_engine_subagent(&files.record, &sessions, first.next)?;
        assert_eq!(again.entries, []);
        assert_eq!(again.next, first.next);
        fs::remove_dir_all(&sessions)
    }
}
