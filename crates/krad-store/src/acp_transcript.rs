use krad_domain::{ChatEntry, EnvironmentName, TerminalId};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;

use crate::chat_images::environment_folder;
use crate::config_dir::config_dir;

const ACP_TRANSCRIPTS_DIR_NAME: &str = "acp-transcripts";
const TRANSCRIPT_EXTENSION: &str = "jsonl";
const WRITER_THREAD_NAME: &str = "krad-ade-acp-transcripts";
const LINE_END: u8 = b'\n';

enum Job {
    Keep(PathBuf, Vec<u8>),
    Forget(PathBuf),
    Stop,
}

fn transcript_file(root: &Path, env: &EnvironmentName, chat: TerminalId) -> PathBuf {
    root.join(ACP_TRANSCRIPTS_DIR_NAME)
        .join(environment_folder(env))
        .join(chat.as_u64().to_string())
        .with_extension(TRANSCRIPT_EXTENSION)
}

#[must_use]
pub fn acp_transcript_file(env: &EnvironmentName, chat: TerminalId) -> Option<PathBuf> {
    config_dir().map(|root| transcript_file(&root, env, chat))
}

/// # Errors
///
/// Returns the I/O error when the transcript exists and cannot be read.
pub fn read_acp_transcript(file: &Path) -> io::Result<Vec<ChatEntry>> {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    Ok(text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}

fn append(file: &Path, lines: &[u8]) -> io::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?
        .write_all(lines)
}

fn forget(file: &Path) -> io::Result<()> {
    match fs::remove_file(file) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        done => done,
    }
}

fn write_kept(kept: HashMap<PathBuf, Vec<u8>>) {
    for (file, lines) in kept {
        drop(append(&file, &lines));
    }
}

fn run(inbox: &Receiver<Job>) {
    while let Ok(first) = inbox.recv() {
        let mut kept: HashMap<PathBuf, Vec<u8>> = HashMap::new();
        let mut stopped = false;
        for job in std::iter::once(first).chain(inbox.try_iter()) {
            match job {
                Job::Keep(file, line) => kept.entry(file).or_default().extend(line),
                Job::Forget(file) => {
                    kept.remove(&file);
                    drop(forget(&file));
                }
                Job::Stop => stopped = true,
            }
        }
        write_kept(kept);
        if stopped {
            return;
        }
    }
}

pub struct AcpTranscripts {
    queue: Option<Sender<Job>>,
    worker: Option<JoinHandle<()>>,
}

impl AcpTranscripts {
    #[must_use]
    pub fn spawn() -> Option<Self> {
        let (queue, inbox) = channel::<Job>();
        let worker = std::thread::Builder::new()
            .name(WRITER_THREAD_NAME.to_owned())
            .spawn(move || run(&inbox))
            .ok()?;
        Some(Self {
            queue: Some(queue),
            worker: Some(worker),
        })
    }

    #[must_use]
    pub fn transcript(&self, file: PathBuf) -> Option<AcpTranscript> {
        Some(AcpTranscript {
            queue: self.queue.clone()?,
            file,
        })
    }

    pub fn forget(&self, file: PathBuf) {
        if let Some(queue) = &self.queue {
            drop(queue.send(Job::Forget(file)));
        }
    }
}

impl Drop for AcpTranscripts {
    fn drop(&mut self) {
        if let Some(queue) = self.queue.take() {
            drop(queue.send(Job::Stop));
        }
        if let Some(worker) = self.worker.take() {
            drop(worker.join());
        }
    }
}

pub struct AcpTranscript {
    queue: Sender<Job>,
    file: PathBuf,
}

impl AcpTranscript {
    pub fn keep(&self, entry: &ChatEntry) {
        let Ok(mut line) = serde_json::to_vec(entry) else {
            return;
        };
        line.push(LINE_END);
        drop(self.queue.send(Job::Keep(self.file.clone(), line)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AgentPreset, ChatEvent};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "krad-acp-transcripts-{name}-{}",
            std::process::id()
        ));
        fs::remove_dir_all(&dir).unwrap_or_default();
        dir
    }

    fn user(text: &str) -> ChatEntry {
        ChatEntry::User {
            text: text.to_owned(),
            preset: AgentPreset::Shell,
            model: None,
            images: Vec::new(),
        }
    }

    #[test]
    fn each_chat_keeps_its_transcript_in_its_environment_folder() {
        let main = EnvironmentName::new("main".to_owned());
        assert_eq!(
            transcript_file(Path::new("/config"), &main, TerminalId(7)),
            Path::new("/config/acp-transcripts/6d61696e/7.jsonl")
        );
    }

    #[test]
    fn kept_entries_read_back_in_order_until_the_chat_is_forgotten() -> io::Result<()> {
        let file = scratch("keep").join("main").join("3.jsonl");
        let entries = [
            user("Fix the race"),
            ChatEntry::Event(ChatEvent::TextDelta("Looking".to_owned())),
            ChatEntry::Event(ChatEvent::Done),
        ];
        let Some(transcripts) = AcpTranscripts::spawn() else {
            return Err(io::Error::other("the writer thread starts"));
        };
        let Some(transcript) = transcripts.transcript(file.clone()) else {
            return Err(io::Error::other("a running writer hands out transcripts"));
        };
        for entry in &entries {
            transcript.keep(entry);
        }
        drop(transcripts);
        assert_eq!(read_acp_transcript(&file)?, entries);
        transcript.keep(&user("After the app stopped"));
        {
            let Some(transcripts) = AcpTranscripts::spawn() else {
                return Err(io::Error::other("the writer thread starts"));
            };
            transcripts.forget(file.clone());
        }
        assert_eq!(read_acp_transcript(&file)?, Vec::new());
        Ok(())
    }

    #[test]
    fn an_unreadable_line_is_skipped() -> io::Result<()> {
        let file = scratch("torn").join("1.jsonl");
        let kept = serde_json::to_string(&user("Hi")).map_err(io::Error::other)?;
        append(&file, format!("{kept}\n{{\"User\":").as_bytes())?;
        assert_eq!(read_acp_transcript(&file)?, vec![user("Hi")]);
        Ok(())
    }
}
