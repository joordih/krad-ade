use crate::config_dir::config_dir;
use krad_domain::{EnvironmentName, EventRecord, GroupId};
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::thread::JoinHandle;

const JOURNAL_MAX_BYTES: u64 = 512 * 1024;
const JOURNAL_ROTATED_FILES: u32 = 3;
const JOURNAL_QUEUE: usize = 256;
const EVENTS_DIR: &str = "events";
const JOURNAL_EXTENSION: &str = "jsonl";
const ENVIRONMENT_JOURNAL: &str = "environment";
const GROUP_JOURNAL_PREFIX: &str = "group-";
const JOURNAL_THREAD_NAME: &str = "krad-ade-event-journal";
const FILE_NAME_REPLACEMENT: char = '_';
const LINE_END: u8 = b'\n';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JournalSend {
    Queued,
    Full,
    Stopped,
}

pub struct EventJournal {
    queue: Option<SyncSender<EventRecord>>,
    worker: Option<JoinHandle<()>>,
}

impl EventJournal {
    #[must_use]
    pub fn spawn() -> Option<Self> {
        Self::spawn_in(config_dir()?.join(EVENTS_DIR))
    }

    fn spawn_in(dir: PathBuf) -> Option<Self> {
        let (queue, inbox) = sync_channel::<EventRecord>(JOURNAL_QUEUE);
        let worker = std::thread::Builder::new()
            .name(JOURNAL_THREAD_NAME.to_owned())
            .spawn(move || {
                while let Ok(record) = inbox.recv() {
                    drop(append(&dir, &record));
                }
            })
            .ok()?;
        Some(Self {
            queue: Some(queue),
            worker: Some(worker),
        })
    }

    #[must_use]
    pub fn record(&self, record: EventRecord) -> JournalSend {
        let Some(queue) = &self.queue else {
            return JournalSend::Stopped;
        };
        match queue.try_send(record) {
            Ok(()) => JournalSend::Queued,
            Err(TrySendError::Full(_)) => JournalSend::Full,
            Err(TrySendError::Disconnected(_)) => JournalSend::Stopped,
        }
    }
}

impl Drop for EventJournal {
    fn drop(&mut self) {
        self.queue = None;
        if let Some(worker) = self.worker.take() {
            drop(worker.join());
        }
    }
}

fn folder_name(environment: &EnvironmentName) -> String {
    environment
        .as_str()
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() || char == '-' {
                char
            } else {
                FILE_NAME_REPLACEMENT
            }
        })
        .collect()
}

fn journal_path(dir: &Path, environment: &EnvironmentName, group: Option<GroupId>) -> PathBuf {
    let stem = group.map_or_else(
        || ENVIRONMENT_JOURNAL.to_owned(),
        |group| format!("{GROUP_JOURNAL_PREFIX}{}", group.as_u64()),
    );
    dir.join(folder_name(environment))
        .join(format!("{stem}.{JOURNAL_EXTENSION}"))
}

fn rotated_path(path: &Path, generation: u32) -> PathBuf {
    path.with_extension(format!("{generation}.{JOURNAL_EXTENSION}"))
}

fn rename_if_present(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

fn rotate(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(rotated_path(path, JOURNAL_ROTATED_FILES)) {
        Err(error) if error.kind() != ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    for generation in (1..JOURNAL_ROTATED_FILES).rev() {
        rename_if_present(
            &rotated_path(path, generation),
            &rotated_path(path, generation + 1),
        )?;
    }
    rename_if_present(path, &rotated_path(path, 1))
}

fn append(dir: &Path, record: &EventRecord) -> std::io::Result<()> {
    let path = journal_path(dir, &record.environment, record.group);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_vec(record)?;
    line.push(LINE_END);
    let size = std::fs::metadata(&path).map_or(0, |meta| meta.len());
    let added = u64::try_from(line.len()).unwrap_or(u64::MAX);
    if size > 0 && size.saturating_add(added) > JOURNAL_MAX_BYTES {
        rotate(&path)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?
        .write_all(&line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AppEvent, SessionId, EVENT_API_VERSION};
    use std::time::{Duration, UNIX_EPOCH};

    fn read_in(
        dir: &Path,
        environment: &EnvironmentName,
        group: Option<GroupId>,
    ) -> Vec<EventRecord> {
        let path = journal_path(dir, environment, group);
        let oldest_first = (1..=JOURNAL_ROTATED_FILES)
            .rev()
            .map(|generation| rotated_path(&path, generation))
            .chain(std::iter::once(path.clone()));
        oldest_first
            .filter_map(|file| std::fs::read(file).ok())
            .flat_map(|bytes| readable_records(&bytes))
            .collect()
    }

    fn readable_records(bytes: &[u8]) -> Vec<EventRecord> {
        String::from_utf8_lossy(bytes)
            .lines()
            .filter_map(EventRecord::read)
            .collect()
    }

    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("krad-ade-journal-{name}-{}", std::process::id()));
            drop(std::fs::remove_dir_all(&dir));
            Self(dir)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            drop(std::fs::remove_dir_all(&self.0));
        }
    }

    fn main_env() -> EnvironmentName {
        EnvironmentName::new("main".to_owned())
    }

    fn record(session: u64) -> EventRecord {
        EventRecord::new(
            UNIX_EPOCH + Duration::from_secs(session),
            main_env(),
            Some(GroupId(1)),
            AppEvent::SessionOpened {
                session: SessionId(session),
            },
        )
    }

    fn session_of(record: &EventRecord) -> Option<u64> {
        match record.event {
            AppEvent::SessionOpened { session } => Some(session.as_u64()),
            _ => None,
        }
    }

    #[test]
    fn records_land_in_their_group_file_in_order() -> std::io::Result<()> {
        let folder = Folder::new("order");
        for session in 1..=3 {
            append(&folder.0, &record(session))?;
        }
        let read = read_in(&folder.0, &main_env(), Some(GroupId(1)));
        let sessions: Vec<Option<u64>> = read.iter().map(session_of).collect();
        assert_eq!(sessions, vec![Some(1), Some(2), Some(3)]);
        assert_eq!(
            read_in(&folder.0, &main_env(), Some(GroupId(2))),
            Vec::new()
        );
        assert_eq!(read_in(&folder.0, &main_env(), None), Vec::new());
        Ok(())
    }

    #[test]
    fn a_full_journal_rotates_and_keeps_a_bounded_history() -> std::io::Result<()> {
        let folder = Folder::new("rotate");
        let line = serde_json::to_vec(&record(1))?.len() + 1;
        let per_file = JOURNAL_MAX_BYTES / u64::try_from(line).unwrap_or(1);
        let written = per_file * u64::from(JOURNAL_ROTATED_FILES + 3);
        for session in 0..written {
            append(&folder.0, &record(session))?;
        }
        let path = journal_path(&folder.0, &main_env(), Some(GroupId(1)));
        for generation in 1..=JOURNAL_ROTATED_FILES {
            let rotated = std::fs::metadata(rotated_path(&path, generation))?;
            assert!(rotated.len() <= JOURNAL_MAX_BYTES);
        }
        assert!(!rotated_path(&path, JOURNAL_ROTATED_FILES + 1).exists());
        let read = read_in(&folder.0, &main_env(), Some(GroupId(1)));
        let kept = u64::try_from(read.len()).unwrap_or(0);
        assert!(kept < written);
        assert!(kept >= per_file * u64::from(JOURNAL_ROTATED_FILES));
        assert_eq!(read.last().and_then(session_of), Some(written - 1));
        let sessions: Vec<Option<u64>> = read.iter().map(session_of).collect();
        assert!(sessions.windows(2).all(|pair| pair[0] < pair[1]));
        Ok(())
    }

    #[test]
    fn the_reader_skips_torn_foreign_and_newer_lines() -> std::io::Result<()> {
        let folder = Folder::new("tolerant");
        append(&folder.0, &record(1))?;
        let path = journal_path(&folder.0, &main_env(), Some(GroupId(1)));
        let mut newer = serde_json::to_value(record(2))?;
        newer["api"] = serde_json::json!(EVENT_API_VERSION.0 + 1);
        let mut file = OpenOptions::new().append(true).open(&path)?;
        file.write_all(b"{\"api\":1,\"at\":\n")?;
        file.write_all(&[0xff, 0xfe, LINE_END])?;
        file.write_all(format!("{newer}\n").as_bytes())?;
        drop(file);
        append(&folder.0, &record(3))?;
        let read = read_in(&folder.0, &main_env(), Some(GroupId(1)));
        let sessions: Vec<Option<u64>> = read.iter().map(session_of).collect();
        assert_eq!(sessions, vec![Some(1), Some(3)]);
        Ok(())
    }

    #[test]
    fn environment_names_cannot_leave_the_events_folder() {
        let path = journal_path(
            Path::new("/events"),
            &EnvironmentName::new("../../etc".to_owned()),
            None,
        );
        assert_eq!(path, Path::new("/events/______etc/environment.jsonl"));
    }

    #[test]
    fn the_writer_thread_appends_what_it_is_sent() {
        let folder = Folder::new("writer");
        let journal = EventJournal::spawn_in(folder.0.clone());
        assert!(journal.is_some(), "a journal thread starts");
        if let Some(journal) = journal {
            assert_eq!(journal.record(record(1)), JournalSend::Queued);
            assert_eq!(journal.record(record(2)), JournalSend::Queued);
        }
        let read = read_in(&folder.0, &main_env(), Some(GroupId(1)));
        assert_eq!(read.len(), 2);
    }
}
