use krad_domain::{DirEntry, EnvironmentName};
use krad_protocol::{ErrorCode, EventKind, FolderChanged, Reply};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub(crate) const WATCH_INTERVAL: Duration = Duration::from_secs(2);
const MAX_WATCHED: usize = 256;
const TOO_MANY: &str = "too many folders are watched";
const NOT_WATCHED: &str = "that folder is not watched";

pub(crate) type Folder = (EnvironmentName, PathBuf);

#[derive(Debug)]
struct Watched {
    watchers: usize,
    seen: Option<Vec<DirEntry>>,
    due: Instant,
    listing: bool,
}

#[derive(Debug, Default)]
pub(crate) struct FolderWatches {
    watched: HashMap<Folder, Watched>,
}

fn refused(code: ErrorCode, message: &str) -> Reply {
    Reply::Refused {
        code,
        message: message.to_owned(),
    }
}

impl FolderWatches {
    pub(crate) fn watch(&mut self, folder: Folder, now: Instant) -> Reply {
        if let Some(watched) = self.watched.get_mut(&folder) {
            watched.watchers += 1;
            watched.due = now;
            watched.seen = None;
            return Reply::Accepted;
        }
        if self.watched.len() >= MAX_WATCHED {
            return refused(ErrorCode::Busy, TOO_MANY);
        }
        self.watched.insert(
            folder,
            Watched {
                watchers: 1,
                seen: None,
                due: now,
                listing: false,
            },
        );
        Reply::Accepted
    }

    pub(crate) fn unwatch(&mut self, folder: &Folder) -> Reply {
        let Some(watched) = self.watched.get_mut(folder) else {
            return refused(ErrorCode::NotFound, NOT_WATCHED);
        };
        watched.watchers -= 1;
        if watched.watchers == 0 {
            self.watched.remove(folder);
        }
        Reply::Accepted
    }

    pub(crate) fn due(&mut self, now: Instant) -> Vec<Folder> {
        self.watched
            .iter_mut()
            .filter(|(_, watched)| !watched.listing && watched.due <= now)
            .map(|(folder, watched)| {
                watched.listing = true;
                folder.clone()
            })
            .collect()
    }

    pub(crate) fn listed(
        &mut self,
        folder: Folder,
        listed: Result<Vec<DirEntry>, String>,
        now: Instant,
        outbox: &mut Vec<EventKind>,
    ) {
        let Some(watched) = self.watched.get_mut(&folder) else {
            return;
        };
        watched.listing = false;
        watched.due = now + WATCH_INTERVAL;
        let entries = match listed {
            Ok(entries) => entries,
            Err(error) => {
                tracing::debug!(%error, "a watched folder could not be listed");
                Vec::new()
            }
        };
        if watched.seen.as_ref() == Some(&entries) {
            return;
        }
        watched.seen = Some(entries.clone());
        let (environment, dir) = folder;
        outbox.push(EventKind::FolderChanged(Box::new(FolderChanged {
            environment,
            dir,
            entries,
        })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::EntryKind;

    fn folder() -> Folder {
        (
            EnvironmentName::new("main".to_owned()),
            PathBuf::from("/home/dev/app"),
        )
    }

    fn entry(name: &str) -> DirEntry {
        DirEntry {
            name: name.to_owned(),
            path: PathBuf::from("/home/dev/app").join(name),
            kind: EntryKind::File,
        }
    }

    #[test]
    fn a_folder_is_announced_once_per_change_and_listed_again_after_the_interval() {
        let mut watches = FolderWatches::default();
        let now = Instant::now();
        assert_eq!(watches.watch(folder(), now), Reply::Accepted);
        assert_eq!(watches.due(now), vec![folder()]);
        assert_eq!(watches.due(now), Vec::<Folder>::new());
        let mut outbox = Vec::new();
        watches.listed(folder(), Ok(vec![entry("a.rs")]), now, &mut outbox);
        watches.due(now + WATCH_INTERVAL);
        watches.listed(
            folder(),
            Ok(vec![entry("a.rs")]),
            now + WATCH_INTERVAL,
            &mut outbox,
        );
        assert_eq!(outbox.len(), 1);
        watches.due(now + WATCH_INTERVAL * 2);
        watches.listed(
            folder(),
            Ok(vec![entry("a.rs"), entry("b.rs")]),
            now,
            &mut outbox,
        );
        assert_eq!(outbox.len(), 2);
    }

    #[test]
    fn a_folder_stays_watched_until_its_last_watcher_leaves() {
        let mut watches = FolderWatches::default();
        let now = Instant::now();
        watches.watch(folder(), now);
        watches.watch(folder(), now);
        assert_eq!(watches.unwatch(&folder()), Reply::Accepted);
        assert_eq!(watches.due(now), vec![folder()]);
        assert_eq!(watches.unwatch(&folder()), Reply::Accepted);
        assert!(matches!(
            watches.unwatch(&folder()),
            Reply::Refused {
                code: ErrorCode::NotFound,
                ..
            }
        ));
    }
}
