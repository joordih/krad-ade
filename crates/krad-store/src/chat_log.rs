use crate::config_dir::config_dir;
use krad_domain::{ChatEntry, ChatId};
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;
use std::time::SystemTime;

const CHATS_DIR_NAME: &str = "chats";
const CHAT_EXTENSION: &str = "jsonl";
const CHAT_WRITER_THREAD_NAME: &str = "krad-ade-chat-writer";

fn chats_dir() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(CHATS_DIR_NAME))
}

fn chat_file(dir: &Path, id: &ChatId) -> PathBuf {
    dir.join(id.as_str()).with_extension(CHAT_EXTENSION)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredChat {
    pub id: ChatId,
    pub entries: Vec<ChatEntry>,
}

#[must_use]
pub fn read_latest_chat() -> Option<StoredChat> {
    read_latest_chat_in(&chats_dir()?)
}

fn read_latest_chat_in(dir: &Path) -> Option<StoredChat> {
    let (path, id) = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != CHAT_EXTENSION {
                return None;
            }
            let id = path.file_stem()?.to_str()?.parse::<ChatId>().ok()?;
            let modified = std::fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            Some((modified, path, id))
        })
        .max_by_key(|(modified, _, _)| *modified)
        .map(|(_, path, id)| (path, id))?;
    read_entries(&path).map(|entries| StoredChat { id, entries })
}

fn read_entries(path: &Path) -> Option<Vec<ChatEntry>> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(
        text.lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect(),
    )
}

pub struct ChatLogWriter {
    queue: Option<Sender<(ChatId, String)>>,
    worker: Option<JoinHandle<()>>,
}

impl ChatLogWriter {
    #[must_use]
    pub fn spawn() -> Option<Self> {
        Self::spawn_in(chats_dir()?)
    }

    fn spawn_in(dir: PathBuf) -> Option<Self> {
        let (queue, inbox) = channel::<(ChatId, String)>();
        let worker = std::thread::Builder::new()
            .name(CHAT_WRITER_THREAD_NAME.to_owned())
            .spawn(move || {
                while let Ok(first) = inbox.recv() {
                    let mut batch = vec![first];
                    batch.extend(inbox.try_iter());
                    for run in batch.chunk_by(|left, right| left.0 == right.0) {
                        let lines: String = run.iter().map(|(_, line)| line.as_str()).collect();
                        drop(append(&dir, &run[0].0, &lines));
                    }
                }
            })
            .ok()?;
        Some(Self {
            queue: Some(queue),
            worker: Some(worker),
        })
    }

    pub fn append(&self, id: &ChatId, entry: &ChatEntry) {
        let (Some(queue), Ok(mut line)) = (&self.queue, serde_json::to_string(entry)) else {
            return;
        };
        line.push('\n');
        drop(queue.send((id.clone(), line)));
    }
}

fn append(dir: &Path, id: &ChatId, lines: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(chat_file(dir, id))?
        .write_all(lines.as_bytes())
}

impl Drop for ChatLogWriter {
    fn drop(&mut self) {
        self.queue = None;
        if let Some(worker) = self.worker.take() {
            drop(worker.join());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AgentPreset, ChatEvent};
    use std::fs::File;
    use std::time::Duration;

    const OLDER_CHAT_AGE: Duration = Duration::from_secs(60);

    fn id(raw: &str) -> ChatId {
        raw.parse()
            .unwrap_or_else(|_| unreachable!("{raw} is a valid id"))
    }

    #[test]
    fn the_latest_chat_reads_back_its_entries_skipping_broken_lines() {
        let dir = std::env::temp_dir().join(format!("krad-ade-chats-{}", std::process::id()));
        let entries = [
            ChatEntry::User {
                text: "hi".to_owned(),
                preset: AgentPreset::ClaudeCode,
                model: None,
            },
            ChatEntry::Event(ChatEvent::Text("hello".to_owned())),
            ChatEntry::Event(ChatEvent::Done),
        ];
        let writer = ChatLogWriter::spawn_in(dir.clone());
        assert!(writer.is_some(), "a writer thread starts");
        if let Some(writer) = writer {
            writer.append(&id("older"), &entries[0]);
            for entry in &entries {
                writer.append(&id("newer"), entry);
            }
        }
        assert!(append(&dir, &id("newer"), "{\"cut short").is_ok());
        let aged = File::options()
            .write(true)
            .open(chat_file(&dir, &id("older")))
            .and_then(|file| file.set_modified(SystemTime::now() - OLDER_CHAT_AGE));
        assert!(aged.is_ok(), "the older chat is dated back");
        let latest = read_latest_chat_in(&dir);
        assert_eq!(
            latest,
            Some(StoredChat {
                id: id("newer"),
                entries: entries.to_vec(),
            })
        );
        assert!(std::fs::remove_dir_all(&dir).is_ok());
    }
}
