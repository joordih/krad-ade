use krad_domain::ToolUseId;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::ChatError;
use crate::history::{read_lines, Sidechain, TranscriptChunk};

pub(crate) const SUBAGENTS_DIR: &str = "subagents";
const RECORD_PREFIX: &str = "agent-";
pub(crate) const RECORD_EXTENSION: &str = "jsonl";
pub(crate) const META_EXTENSION: &str = "meta.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentMeta {
    pub tool_use_id: ToolUseId,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub finished: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubagentFiles {
    pub record: PathBuf,
    pub meta: PathBuf,
}

#[must_use]
pub fn claude_subagents(conversation: &Path) -> Vec<SubagentFiles> {
    let folder = conversation.with_extension("").join(SUBAGENTS_DIR);
    let Ok(entries) = fs::read_dir(&folder) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == RECORD_EXTENSION)
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(RECORD_PREFIX))
        })
        .map(|record| SubagentFiles {
            meta: record.with_extension(META_EXTENSION),
            record,
        })
        .collect()
}

/// # Errors
///
/// Fails when the note cannot be read or is not the note Claude Code writes.
pub fn read_subagent_meta(file: &Path) -> Result<SubagentMeta, ChatError> {
    let text = fs::read_to_string(file)?;
    serde_json::from_str(&text).map_err(|error| ChatError::Io(io::Error::other(error)))
}

/// # Errors
///
/// Returns the I/O error when the record cannot be opened or read.
pub fn read_subagent_record(file: &Path, from: u64) -> io::Result<TranscriptChunk> {
    read_lines(file, from, Sidechain::Kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{ChatEntry, ChatEvent};

    fn folder(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("krad-subagents-{name}-{}", std::process::id()))
    }

    #[test]
    fn a_conversation_finds_its_subagents_and_their_notes() -> Result<(), ChatError> {
        let root = folder("find");
        let conversation = root.join("0b7c3a52.jsonl");
        let subagents = root.join("0b7c3a52").join(SUBAGENTS_DIR);
        fs::create_dir_all(&subagents)?;
        fs::write(&conversation, "")?;
        let record = subagents.join("agent-a1.jsonl");
        fs::write(
            &record,
            concat!(
                r#"{"type":"user","isSidechain":true,"message":{"role":"user","content":"Map the UI"}}"#,
                "\n",
                r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","id":"t9","name":"Grep","input":{"pattern":"render"}}]}}"#,
                "\n",
            ),
        )?;
        fs::write(
            subagents.join("agent-a1.meta.json"),
            r#"{"agentType":"Explore","description":"Map the UI","toolUseId":"toolu_1","finished":true}"#,
        )?;
        fs::write(subagents.join("notes.txt"), "not a subagent")?;
        let found = claude_subagents(&conversation);
        assert_eq!(found.len(), 1);
        let meta = read_subagent_meta(&found[0].meta)?;
        assert_eq!(meta.tool_use_id, ToolUseId::from("toolu_1"));
        assert_eq!(meta.agent_type.as_deref(), Some("Explore"));
        assert!(meta.finished);
        let chunk = read_subagent_record(&found[0].record, 0)?;
        assert!(matches!(&chunk.entries[0], ChatEntry::User { text, .. } if text == "Map the UI"));
        assert!(matches!(
            &chunk.entries[1],
            ChatEntry::Event(ChatEvent::Tool(call)) if call.name == "Grep"
        ));
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn a_conversation_without_subagents_has_none() {
        assert_eq!(
            claude_subagents(&folder("none").join("x.jsonl")),
            Vec::new()
        );
    }
}
