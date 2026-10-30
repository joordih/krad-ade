use krad_domain::{AgentPreset, ChatEntry, ChatEvent, ResumeId, RewindId, RewindPoint};
use serde_json::Value;
use std::fs;
use std::io::{self, Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};

use crate::parse::{claude, permission_mode};
use crate::skills::CLAUDE_DIR;

const PROJECTS_DIR: &str = "projects";
const CONVERSATION_EXTENSION: &str = "jsonl";
const TEXT_BLOCK: &str = "text";
const TOOL_RESULT_BLOCK: &str = "tool_result";
const BLOCK_SEPARATOR: &str = "\n\n";
const LINE_END: u8 = b'\n';
const META_FLAG: &str = "/isMeta";
const SIDECHAIN_FLAG: &str = "/isSidechain";
const PERMISSION_MODE_LINE: &str = "permission-mode";
const TAGGED_TEXT_START: char = '<';
const USER_LINE: &str = "user";
const UUID_KEY: &str = "uuid";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TranscriptChunk {
    pub entries: Vec<ChatEntry>,
    pub next: u64,
}

#[must_use]
pub fn claude_conversation_file(home: &Path, id: &ResumeId) -> Option<PathBuf> {
    conversation_file(&home.join(CLAUDE_DIR).join(PROJECTS_DIR), id)
}

/// # Errors
///
/// Returns the I/O error when the file cannot be opened or read.
pub fn read_claude_transcript(file: &Path, from: u64) -> io::Result<TranscriptChunk> {
    read_lines(file, from, Sidechain::Skipped)
}

/// # Errors
///
/// Returns the I/O error when the file cannot be opened or read.
pub fn read_claude_transcript_tail(
    file: &Path,
    max_bytes: u64,
) -> io::Result<(TranscriptChunk, u64)> {
    let length = fs::metadata(file)?.len();
    let mut start = length.saturating_sub(max_bytes);
    if start > 0 {
        let mut reader = fs::File::open(file)?;
        reader.seek(SeekFrom::Start(start))?;
        let mut cut = Vec::new();
        io::Read::by_ref(&mut reader)
            .take(max_bytes)
            .read_to_end(&mut cut)?;
        let Some(line_end) = cut.iter().position(|byte| *byte == LINE_END) else {
            return Ok((
                TranscriptChunk {
                    entries: Vec::new(),
                    next: start,
                },
                start,
            ));
        };
        start += u64::try_from(line_end + 1).unwrap_or(u64::MAX);
    }
    Ok((read_lines(file, start, Sidechain::Skipped)?, start))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sidechain {
    Skipped,
    Kept,
}

pub(crate) fn read_lines(
    file: &Path,
    from: u64,
    sidechain: Sidechain,
) -> io::Result<TranscriptChunk> {
    let mut reader = fs::File::open(file)?;
    reader.seek(SeekFrom::Start(from))?;
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    let Some(end) = bytes.iter().rposition(|byte| *byte == LINE_END) else {
        return Ok(TranscriptChunk {
            entries: Vec::new(),
            next: from,
        });
    };
    Ok(TranscriptChunk {
        entries: entries_of_bytes(&bytes[..end], sidechain),
        next: from + u64::try_from(end + 1).unwrap_or(u64::MAX),
    })
}

/// # Errors
///
/// Returns the I/O error when the file cannot be opened or read.
pub fn read_claude_transcript_before(file: &Path, end: u64) -> io::Result<Vec<ChatEntry>> {
    let mut bytes = Vec::new();
    fs::File::open(file)?.take(end).read_to_end(&mut bytes)?;
    Ok(entries_of_bytes(&bytes, Sidechain::Skipped))
}

fn entries_of_bytes(bytes: &[u8], sidechain: Sidechain) -> Vec<ChatEntry> {
    String::from_utf8_lossy(bytes)
        .lines()
        .flat_map(|line| entries_of_line(line, sidechain))
        .collect()
}

fn conversation_file(projects: &Path, id: &ResumeId) -> Option<PathBuf> {
    fs::read_dir(projects)
        .ok()?
        .filter_map(Result::ok)
        .map(|project| {
            project
                .path()
                .join(id.as_str())
                .with_extension(CONVERSATION_EXTENSION)
        })
        .find(|file| file.is_file())
}

fn entries_of_line(line: &str, sidechain: Sidechain) -> Vec<ChatEntry> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    let flagged = |flag: &str| value.pointer(flag).and_then(Value::as_bool) == Some(true);
    if flagged(META_FLAG) || (sidechain == Sidechain::Skipped && flagged(SIDECHAIN_FLAG)) {
        return Vec::new();
    }
    match value.get("type").and_then(Value::as_str) {
        Some(USER_LINE) => match user_text(&value) {
            Some(text) => std::iter::once(ChatEntry::User {
                text,
                preset: AgentPreset::ClaudeCode,
                model: None,
                images: Vec::new(),
            })
            .chain(prompted(&value))
            .collect(),
            None => claude(&value).into_iter().map(ChatEntry::Event).collect(),
        },
        Some("assistant") => claude(&value).into_iter().map(ChatEntry::Event).collect(),
        Some(PERMISSION_MODE_LINE) => permission_mode(&value)
            .into_iter()
            .map(ChatEntry::Event)
            .collect(),
        _ => Vec::new(),
    }
}

fn prompted(value: &Value) -> Option<ChatEntry> {
    let uuid = value.get(UUID_KEY).and_then(Value::as_str)?;
    Some(ChatEntry::Event(ChatEvent::Prompted(RewindPoint::Message(
        RewindId::from(uuid),
    ))))
}

pub(crate) fn is_prompt(value: &Value) -> bool {
    let flagged = |flag: &str| value.pointer(flag).and_then(Value::as_bool) == Some(true);
    value.get("type").and_then(Value::as_str) == Some(USER_LINE)
        && !flagged(META_FLAG)
        && !flagged(SIDECHAIN_FLAG)
        && user_text(value).is_some()
}

fn block_type(block: &Value) -> Option<&str> {
    block.get("type").and_then(Value::as_str)
}

fn user_text(value: &Value) -> Option<String> {
    let text = match value.pointer("/message/content")? {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => {
            if blocks
                .iter()
                .any(|block| block_type(block) == Some(TOOL_RESULT_BLOCK))
            {
                return None;
            }
            blocks
                .iter()
                .filter(|block| block_type(block) == Some(TEXT_BLOCK))
                .filter_map(|block| block.get(TEXT_BLOCK).and_then(Value::as_str))
                .collect::<Vec<&str>>()
                .join(BLOCK_SEPARATOR)
        }
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty() && !text.starts_with(TAGGED_TEXT_START)).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{PermissionMode, ToolCall};

    const CONVERSATION: &str = r#"{"type":"permission-mode","permissionMode":"auto"}
{"type":"user","message":{"role":"user","content":"Verify the Stripe signature."}}
{"type":"user","isMeta":true,"message":{"role":"user","content":"Caveat: injected"}}
{"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>"}}
{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"hm"}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"webhook.ts"}}]}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"..."}]}}
{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"subagent"}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Done."}]}}
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Thanks"}]}}
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Look [Image #1]"},{"type":"image","source":{}}]}}
not json"#;

    fn user(text: &str) -> ChatEntry {
        ChatEntry::User {
            text: text.to_owned(),
            preset: AgentPreset::ClaudeCode,
            model: None,
            images: Vec::new(),
        }
    }

    #[test]
    fn a_tail_read_starts_at_a_whole_line() -> io::Result<()> {
        let file = std::env::temp_dir().join(format!("krad-tail-{}.jsonl", std::process::id()));
        let first = r#"{"type":"user","message":{"content":"First question"}}"#;
        let second = r#"{"type":"user","message":{"content":"Second question"}}"#;
        fs::write(&file, format!("{first}\n{second}\n"))?;
        let cut = u64::try_from(second.len() + 5).unwrap_or(u64::MAX);
        let (chunk, start) = read_claude_transcript_tail(&file, cut)?;
        assert_eq!(start, u64::try_from(first.len() + 1).unwrap_or(u64::MAX));
        assert!(matches!(
            chunk.entries.as_slice(),
            [ChatEntry::User { text, .. }] if text == "Second question"
        ));
        let earlier = read_claude_transcript_before(&file, start)?;
        assert!(matches!(
            earlier.as_slice(),
            [ChatEntry::User { text, .. }] if text == "First question"
        ));
        let (whole, from) = read_claude_transcript_tail(&file, u64::MAX)?;
        assert_eq!((whole.entries.len(), from), (2, 0));
        fs::remove_file(file)
    }

    #[test]
    fn a_conversation_keeps_what_the_user_wrote_and_the_answers() {
        let entries: Vec<ChatEntry> = CONVERSATION
            .lines()
            .flat_map(|line| entries_of_line(line, Sidechain::Skipped))
            .collect();
        assert_eq!(
            entries,
            [
                ChatEntry::Event(ChatEvent::PermissionMode(PermissionMode::Auto)),
                user("Verify the Stripe signature."),
                ChatEntry::Event(ChatEvent::Thinking("hm".to_owned())),
                ChatEntry::Event(ChatEvent::Tool(ToolCall {
                    id: None,
                    name: "Read".to_owned(),
                    summary: "webhook.ts".to_owned(),
                    edit: None,
                })),
                ChatEntry::Event(ChatEvent::Text("Done.".to_owned())),
                user("Thanks"),
                user("Look [Image #1]"),
            ]
        );
    }

    #[test]
    fn a_prompt_with_a_uuid_can_be_rewound_to() {
        let line = r#"{"type":"user","uuid":"u1","parentUuid":null,"message":{"role":"user","content":"Fix it"}}"#;
        assert_eq!(
            entries_of_line(line, Sidechain::Skipped),
            [
                user("Fix it"),
                ChatEntry::Event(ChatEvent::Prompted(RewindPoint::Message(RewindId::from(
                    "u1"
                )))),
            ]
        );
        let value: Value = serde_json::from_str(line).unwrap_or_default();
        assert!(is_prompt(&value));
        let result: Value = serde_json::from_str(
            r#"{"type":"user","uuid":"r1","message":{"content":[{"type":"tool_result","content":"ok"}]}}"#,
        )
        .unwrap_or_default();
        assert!(!is_prompt(&result));
    }

    #[test]
    fn the_conversation_is_found_in_any_project_folder() {
        let home = std::env::temp_dir().join(format!("krad-ade-history-{}", std::process::id()));
        let project = home.join(CLAUDE_DIR).join(PROJECTS_DIR).join("-srv-api");
        let Ok(id) = "0b7c3a52-9f1e".parse::<ResumeId>() else {
            unreachable!("a valid resume id");
        };
        assert!(fs::create_dir_all(&project).is_ok());
        let file = project
            .join(id.as_str())
            .with_extension(CONVERSATION_EXTENSION);
        assert!(fs::write(&file, CONVERSATION).is_ok());
        assert_eq!(claude_conversation_file(&home, &id), Some(file));
        let Ok(other) = "1111".parse::<ResumeId>() else {
            unreachable!("a valid resume id");
        };
        assert_eq!(claude_conversation_file(&home, &other), None);
        assert!(fs::remove_dir_all(&home).is_ok());
    }

    #[test]
    fn the_transcript_is_read_a_complete_line_at_a_time() {
        let dir = std::env::temp_dir().join(format!("krad-ade-transcript-{}", std::process::id()));
        assert!(fs::create_dir_all(&dir).is_ok());
        let file = dir.join("conversation.jsonl");
        let asked = r#"{"type":"user","message":{"role":"user","content":"Hi"}}"#;
        let half = r#"{"type":"assistant","message":{"content":[{"type":"te"#;
        assert!(fs::write(&file, format!("{asked}\n{half}")).is_ok());
        let first = read_claude_transcript(&file, 0).unwrap_or_default();
        assert_eq!(first.entries, [user("Hi")]);
        let rest = r#"xt","text":"Hello"}]}}"#;
        assert!(fs::write(&file, format!("{asked}\n{half}{rest}\n")).is_ok());
        let second = read_claude_transcript(&file, first.next).unwrap_or_default();
        assert_eq!(
            second.entries,
            [ChatEntry::Event(ChatEvent::Text("Hello".to_owned()))]
        );
        assert_eq!(
            read_claude_transcript(&file, second.next)
                .unwrap_or_default()
                .entries,
            []
        );
        assert!(fs::remove_dir_all(&dir).is_ok());
    }
}
