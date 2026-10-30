use krad_domain::ResumeId;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use super::subagents::subagents_dir;
use super::wire::Message;
use crate::gateway::Vendor;
use crate::live::{new_session_id, LiveSession};
use crate::refused::ResumeRefused;
use crate::rewind::ResumeCut;
use crate::subagents::SUBAGENTS_DIR;

const SESSION_EXTENSION: &str = "jsonl";
const SNAPSHOTS_DIR: &str = "snapshots";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Record {
    Message(Message),
    Model {
        model: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        vendor: Option<Vendor>,
    },
    Compacted {
        summary: String,
        kept: usize,
    },
    Rewound {
        prompt: usize,
    },
}

impl Record {
    const fn is_prompt(&self) -> bool {
        matches!(self, Self::Message(Message::User { .. }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Chosen {
    pub(crate) id: String,
    pub(crate) vendor: Vendor,
}

impl From<&Chosen> for Record {
    fn from(chosen: &Chosen) -> Self {
        Self::Model {
            model: chosen.id.clone(),
            vendor: Some(chosen.vendor),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Restored {
    pub(crate) messages: Vec<Message>,
    pub(crate) model: Option<String>,
    pub(crate) vendor: Option<Vendor>,
    pub(crate) prompts: usize,
}

pub(crate) fn effective(records: Vec<Record>) -> Vec<Record> {
    let mut kept: Vec<Record> = Vec::new();
    for record in records {
        let Record::Rewound { prompt } = record else {
            kept.push(record);
            continue;
        };
        let cut = kept
            .iter()
            .enumerate()
            .filter(|(_, record)| record.is_prompt())
            .nth(prompt)
            .map_or(kept.len(), |(at, _)| at);
        let cut_off = kept.split_off(cut);
        kept.extend(
            cut_off
                .into_iter()
                .filter(|record| matches!(record, Record::Model { .. })),
        );
    }
    kept
}

impl Restored {
    pub(crate) fn holds(&self, chosen: &Chosen) -> bool {
        self.model.as_ref() == Some(&chosen.id) && self.vendor == Some(chosen.vendor)
    }

    fn apply(&mut self, record: Record) {
        match record {
            Record::Message(message) => {
                self.prompts += usize::from(matches!(message, Message::User { .. }));
                self.messages.push(message);
            }
            Record::Model { model, vendor } => {
                self.model = Some(model);
                self.vendor = vendor;
            }
            Record::Rewound { .. } => {}
            Record::Compacted { summary, kept } => {
                let start = self.messages.len().saturating_sub(kept);
                let recent = self.messages.split_off(start);
                self.messages = vec![Message::user(summary)];
                self.messages.extend(recent);
            }
        }
    }
}

pub(crate) struct SessionFile {
    pub(crate) id: ResumeId,
    pub(crate) subagents: PathBuf,
    pub(crate) snapshots: PathBuf,
    pub(crate) path: PathBuf,
    file: File,
}

pub(crate) fn session_path(dir: &Path, id: &ResumeId) -> PathBuf {
    dir.join(id.as_str()).with_extension(SESSION_EXTENSION)
}

pub(crate) fn read_records(path: &Path) -> io::Result<Vec<Record>> {
    let file = File::open(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => io::Error::new(
            io::ErrorKind::NotFound,
            ResumeRefused::missing(
                path.file_stem()
                    .map_or_default(|stem| stem.to_string_lossy().into_owned()),
            ),
        ),
        _ => error,
    })?;
    let mut records = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        if let Ok(record) = serde_json::from_str::<Record>(&line) {
            records.push(record);
        }
    }
    Ok(records)
}

pub(crate) fn refusal(error: &io::Error) -> Option<ResumeRefused> {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<ResumeRefused>())
        .cloned()
}

pub(crate) fn restore(records: Vec<Record>) -> Restored {
    let mut restored = Restored::default();
    for record in effective(records) {
        restored.apply(record);
    }
    restored
}

fn append_to(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

impl SessionFile {
    pub(crate) fn open(dir: &Path, session: &LiveSession) -> io::Result<(Self, Restored)> {
        fs::create_dir_all(dir)?;
        match session {
            LiveSession::Start(id) => {
                let path = session_path(dir, id);
                Ok((Self::at(dir, id.clone(), path)?, Restored::default()))
            }
            LiveSession::Resume(id) | LiveSession::Cut(ResumeCut { from: id, .. }) => {
                let path = session_path(dir, id);
                let restored = restore(read_records(&path)?);
                Ok((Self::at(dir, id.clone(), path)?, restored))
            }
            LiveSession::Fork(id) => {
                let records = read_records(&session_path(dir, id))?;
                let copy = new_session_id();
                let path = session_path(dir, &copy);
                let mut session = Self::at(dir, copy, path)?;
                for record in &records {
                    session.append(record)?;
                }
                Ok((session, restore(records)))
            }
        }
    }

    fn at(dir: &Path, id: ResumeId, path: PathBuf) -> io::Result<Self> {
        Ok(Self {
            subagents: subagents_dir(dir, &id),
            snapshots: dir.join(id.as_str()).join(SNAPSHOTS_DIR),
            file: append_to(&path)?,
            path,
            id,
        })
    }

    pub(crate) fn create(path: &Path, id: ResumeId) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let folder = path.with_extension("");
        Ok(Self {
            subagents: folder.join(SUBAGENTS_DIR),
            snapshots: folder.join(SNAPSHOTS_DIR),
            file: append_to(path)?,
            path: path.to_path_buf(),
            id,
        })
    }

    pub(crate) fn append(&mut self, record: &Record) -> io::Result<()> {
        let mut line = serde_json::to_string(record).map_err(io::Error::other)?;
        line.push('\n');
        self.file.write_all(line.as_bytes())?;
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::wire::{Block, Stop, Usage};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("krad-engine-session")
            .join(format!("{name}-{}", new_session_id().as_str()));
        fs::create_dir_all(&dir).unwrap_or_default();
        dir
    }

    fn answer(text: &str) -> Message {
        Message::Assistant {
            blocks: vec![Block::Text {
                text: text.to_owned(),
            }],
            usage: Usage::default(),
            stop: Stop::Finished,
        }
    }

    #[test]
    fn a_resumed_session_reads_back_its_messages_and_model() -> io::Result<()> {
        let dir = scratch("resume");
        let id = new_session_id();
        let (mut started, fresh) = SessionFile::open(&dir, &LiveSession::Start(id.clone()))?;
        assert_eq!(fresh, Restored::default());
        started.append(&Record::Message(Message::user("hi".to_owned())))?;
        started.append(&Record::Model {
            model: "gpt-5".to_owned(),
            vendor: Some(Vendor::OpenAi),
        })?;
        started.append(&Record::Message(answer("hello")))?;
        let (_, restored) = SessionFile::open(&dir, &LiveSession::Resume(id))?;
        assert_eq!(restored.model.as_deref(), Some("gpt-5"));
        assert_eq!(restored.vendor, Some(Vendor::OpenAi));
        assert_eq!(restored.messages.len(), 2);
        Ok(())
    }

    #[test]
    fn a_fork_copies_the_history_into_a_new_session() -> io::Result<()> {
        let dir = scratch("fork");
        let id = new_session_id();
        let (mut started, _) = SessionFile::open(&dir, &LiveSession::Start(id.clone()))?;
        started.append(&Record::Message(Message::user("hi".to_owned())))?;
        let (fork, restored) = SessionFile::open(&dir, &LiveSession::Fork(id.clone()))?;
        assert_ne!(fork.id, id);
        assert_eq!(restored.messages.len(), 1);
        assert_eq!(read_records(&session_path(&dir, &fork.id))?.len(), 1);
        Ok(())
    }

    #[test]
    fn a_compaction_keeps_the_summary_and_the_recent_messages() {
        let restored = restore(vec![
            Record::Message(Message::user("one".to_owned())),
            Record::Message(answer("two")),
            Record::Message(Message::user("three".to_owned())),
            Record::Compacted {
                summary: "summary".to_owned(),
                kept: 1,
            },
        ]);
        assert_eq!(
            restored.messages,
            [
                Message::user("summary".to_owned()),
                Message::user("three".to_owned())
            ]
        );
    }

    #[test]
    fn a_rewind_cuts_the_conversation_back_before_the_edited_prompt() -> io::Result<()> {
        let dir = scratch("rewind");
        let id = new_session_id();
        let (mut started, _) = SessionFile::open(&dir, &LiveSession::Start(id.clone()))?;
        for record in [
            Record::Message(Message::user("one".to_owned())),
            Record::Message(answer("1")),
            Record::Message(Message::user("two".to_owned())),
            Record::Model {
                model: "gpt-5".to_owned(),
                vendor: None,
            },
            Record::Message(answer("2")),
            Record::Rewound { prompt: 1 },
            Record::Message(Message::user("two, edited".to_owned())),
        ] {
            started.append(&record)?;
        }
        let (_, restored) = SessionFile::open(&dir, &LiveSession::Resume(id))?;
        assert_eq!(
            restored.messages,
            [
                Message::user("one".to_owned()),
                answer("1"),
                Message::user("two, edited".to_owned())
            ]
        );
        assert_eq!(restored.model.as_deref(), Some("gpt-5"));
        assert_eq!(restored.prompts, 2);
        Ok(())
    }

    #[test]
    fn a_rewind_before_a_compaction_brings_back_the_messages_it_summed_up() {
        let restored = restore(vec![
            Record::Message(Message::user("one".to_owned())),
            Record::Message(answer("1")),
            Record::Message(Message::user("two".to_owned())),
            Record::Compacted {
                summary: "summary".to_owned(),
                kept: 0,
            },
            Record::Rewound { prompt: 1 },
        ]);
        assert_eq!(
            restored.messages,
            [Message::user("one".to_owned()), answer("1")]
        );
        let line = serde_json::to_string(&Record::Rewound { prompt: 1 }).unwrap_or_default();
        assert_eq!(line, r#"{"kind":"rewound","prompt":1}"#);
    }

    #[test]
    fn a_model_saved_before_vendors_still_loads_without_one() {
        let line = r#"{"kind":"model","model":"gpt-5"}"#;
        assert_eq!(
            serde_json::from_str::<Record>(line).ok(),
            Some(Record::Model {
                model: "gpt-5".to_owned(),
                vendor: None
            })
        );
        let saved = serde_json::to_string(&Record::from(&Chosen {
            id: "claude-x".to_owned(),
            vendor: Vendor::Anthropic,
        }))
        .unwrap_or_default();
        assert_eq!(
            saved,
            r#"{"kind":"model","model":"claude-x","vendor":"anthropic"}"#
        );
    }

    #[test]
    fn a_user_message_saved_before_images_still_loads() {
        let line = r#"{"kind":"message","role":"user","text":"hi"}"#;
        assert_eq!(
            serde_json::from_str::<Record>(line).ok(),
            Some(Record::Message(Message::user("hi".to_owned())))
        );
        let saved = serde_json::to_string(&Record::Message(Message::user("hi".to_owned())))
            .unwrap_or_default();
        assert!(!saved.contains("images"), "{saved}");
    }

    #[test]
    fn a_missing_session_is_refused_as_a_missing_conversation() {
        let dir = scratch("missing");
        let refused = SessionFile::open(&dir, &LiveSession::Resume(new_session_id()))
            .err()
            .as_ref()
            .and_then(refusal);
        assert!(
            matches!(refused, Some(ResumeRefused::MissingConversation(_))),
            "{refused:?}"
        );
    }
}
