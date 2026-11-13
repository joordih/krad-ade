use crate::config_dir::config_dir;
use serde::Serialize;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;

mod opening;

pub use opening::{open_layout, open_layout_in, LayoutOpening, OpenedLayout};

const STATE_FILE_NAME: &str = "state.json";
const VIEW_FILE_NAME: &str = "view.json";
const TMP_EXTENSION: &str = "json.tmp";
const PERSIST_THREAD_NAME: &str = "krad-ade-layout-writer";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutFile {
    State,
    View,
}

impl LayoutFile {
    #[must_use]
    pub fn path_in(self, dir: &Path) -> PathBuf {
        dir.join(match self {
            Self::State => STATE_FILE_NAME,
            Self::View => VIEW_FILE_NAME,
        })
    }
}

pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let nonce = RandomState::new().hash_one(std::thread::current().id());
    let tmp = path.with_extension(format!(
        "{TMP_EXTENSION}.{}.{nonce:016x}",
        std::process::id()
    ));
    let written = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        drop(std::fs::remove_file(&tmp));
    }
    written
}

type Written = Box<dyn FnOnce() + Send>;

enum Job {
    Write(Vec<u8>),
    Then(Written),
}

#[derive(Debug)]
pub struct LayoutWriter {
    queue: Option<Sender<Job>>,
    worker: Option<JoinHandle<()>>,
}

fn write_jobs(path: &Path, inbox: &Receiver<Job>, report: &dyn Fn(std::io::Error)) {
    while let Ok(first) = inbox.recv() {
        let mut newest = None;
        let mut waiting = Vec::new();
        for job in std::iter::once(first).chain(inbox.try_iter()) {
            match job {
                Job::Write(bytes) => newest = Some(bytes),
                Job::Then(done) => waiting.push(done),
            }
        }
        if let Some(Err(error)) = newest.map(|bytes| write_atomically(path, &bytes)) {
            report(error);
        }
        for done in waiting {
            done();
        }
    }
}

impl LayoutWriter {
    #[must_use]
    pub fn spawn(file: LayoutFile) -> Option<Self> {
        Self::spawn_at(file.path_in(&config_dir()?))
    }

    #[must_use]
    pub fn spawn_at(path: PathBuf) -> Option<Self> {
        Self::spawn_reporting(path, drop)
    }

    #[must_use]
    pub fn spawn_reporting(
        path: PathBuf,
        report: impl Fn(std::io::Error) + Send + 'static,
    ) -> Option<Self> {
        let (queue, inbox) = channel::<Job>();
        let worker = std::thread::Builder::new()
            .name(PERSIST_THREAD_NAME.to_owned())
            .spawn(move || write_jobs(&path, &inbox, &report))
            .ok()?;
        Some(Self {
            queue: Some(queue),
            worker: Some(worker),
        })
    }

    pub fn persist<T: Serialize>(&self, snapshot: &T) {
        let (Some(queue), Ok(bytes)) = (&self.queue, serde_json::to_vec_pretty(snapshot)) else {
            return;
        };
        drop(queue.send(Job::Write(bytes)));
    }

    pub fn then(&self, done: impl FnOnce() + Send + 'static) {
        let Some(queue) = &self.queue else {
            done();
            return;
        };
        if let Err(unsent) = queue.send(Job::Then(Box::new(done))) {
            if let Job::Then(done) = unsent.0 {
                done();
            }
        }
    }
}

impl Drop for LayoutWriter {
    fn drop(&mut self) {
        self.queue = None;
        if let Some(worker) = self.worker.take() {
            drop(worker.join());
        }
    }
}

#[cfg(test)]
mod tests;
