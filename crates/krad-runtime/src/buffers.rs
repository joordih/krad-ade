use krad_domain::{BufferVersion, EditRefusal, EnvironmentName, TextEdit, VersionedText};
use krad_protocol::{
    BufferEdited, BufferId, BufferNotOpened, ErrorCode, EventKind, OpenedBuffer, Reply, RequestId,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const NO_BUFFER: &str = "no buffer is open with that id";

type Place = (EnvironmentName, PathBuf);

#[derive(Debug)]
struct Open {
    environment: EnvironmentName,
    path: PathBuf,
    text: VersionedText,
    saved: BufferVersion,
}

#[derive(Debug)]
pub(crate) struct Saving {
    pub(crate) buffer: BufferId,
    pub(crate) environment: EnvironmentName,
    pub(crate) path: PathBuf,
    pub(crate) text: String,
    pub(crate) version: BufferVersion,
}

#[derive(Debug, Default)]
pub(crate) struct Buffers {
    last: u64,
    open: HashMap<BufferId, Open>,
    by_place: HashMap<Place, BufferId>,
    opening: HashMap<Place, Vec<RequestId>>,
}

fn refused(code: ErrorCode, message: &str) -> Reply {
    Reply::Refused {
        code,
        message: message.to_owned(),
    }
}

impl Open {
    fn shown(&self, buffer: BufferId) -> OpenedBuffer {
        OpenedBuffer {
            buffer,
            environment: self.environment.clone(),
            path: self.path.clone(),
            version: self.text.version(),
            saved: self.saved,
            text: self.text.text().to_owned(),
        }
    }
}

impl Buffers {
    pub(crate) fn shown(&self, environment: &EnvironmentName, path: &Path) -> Option<EventKind> {
        let place = (environment.clone(), path.to_path_buf());
        let buffer = *self.by_place.get(&place)?;
        let open = self.open.get(&buffer)?;
        Some(EventKind::BufferOpened(Box::new(open.shown(buffer))))
    }

    pub(crate) fn begin_open(
        &mut self,
        environment: &EnvironmentName,
        path: &Path,
        request: RequestId,
    ) -> bool {
        let waiting = self
            .opening
            .entry((environment.clone(), path.to_path_buf()))
            .or_default();
        waiting.push(request);
        waiting.len() == 1
    }

    pub(crate) fn read(
        &mut self,
        (environment, path): Place,
        read: Result<String, String>,
        outbox: &mut Vec<EventKind>,
    ) -> Option<BufferId> {
        let place = (environment, path);
        let waiting = self.opening.remove(&place).unwrap_or_default();
        let (environment, path) = place;
        let text = match read {
            Ok(text) => text,
            Err(message) => {
                outbox.extend(waiting.into_iter().map(|request| {
                    EventKind::BufferNotOpened(Box::new(BufferNotOpened {
                        request,
                        environment: environment.clone(),
                        path: path.clone(),
                        message: message.clone(),
                    }))
                }));
                return None;
            }
        };
        self.last += 1;
        let buffer = BufferId(self.last);
        let open = Open {
            environment: environment.clone(),
            path: path.clone(),
            text: VersionedText::new(text),
            saved: BufferVersion::default(),
        };
        outbox.push(EventKind::BufferOpened(Box::new(open.shown(buffer))));
        self.by_place.insert((environment, path), buffer);
        self.open.insert(buffer, open);
        Some(buffer)
    }

    pub(crate) fn document(&self, buffer: BufferId) -> Option<(EnvironmentName, PathBuf, String)> {
        let open = self.open.get(&buffer)?;
        Some((
            open.environment.clone(),
            open.path.clone(),
            open.text.text().to_owned(),
        ))
    }

    pub(crate) fn edit(
        &mut self,
        buffer: BufferId,
        (based_on, edit): (BufferVersion, &TextEdit),
        author: RequestId,
        outbox: &mut Vec<EventKind>,
    ) -> Reply {
        let Some(open) = self.open.get_mut(&buffer) else {
            return refused(ErrorCode::NotFound, NO_BUFFER);
        };
        match open.text.apply(based_on, edit) {
            Ok(applied) => {
                outbox.push(EventKind::BufferEdited(Box::new(BufferEdited {
                    buffer,
                    version: open.text.version(),
                    edit: applied,
                    author,
                })));
                Reply::Accepted
            }
            Err(refusal @ (EditRefusal::Stale | EditRefusal::Ahead)) => {
                refused(ErrorCode::Invalid, &refusal.to_string())
            }
        }
    }

    pub(crate) fn saving(&self, buffer: BufferId) -> Option<Saving> {
        let open = self.open.get(&buffer)?;
        Some(Saving {
            buffer,
            environment: open.environment.clone(),
            path: open.path.clone(),
            text: open.text.text().to_owned(),
            version: open.text.version(),
        })
    }

    pub(crate) fn saved(
        &mut self,
        buffer: BufferId,
        version: BufferVersion,
        outbox: &mut Vec<EventKind>,
    ) {
        let Some(open) = self.open.get_mut(&buffer) else {
            return;
        };
        open.saved = open.saved.max(version);
        outbox.push(EventKind::BufferSaved { buffer, version });
    }

    pub(crate) fn close(&mut self, buffer: BufferId, outbox: &mut Vec<EventKind>) -> Reply {
        let Some(open) = self.open.remove(&buffer) else {
            return refused(ErrorCode::NotFound, NO_BUFFER);
        };
        self.by_place.remove(&(open.environment, open.path));
        outbox.push(EventKind::BufferClosed { buffer });
        Reply::Accepted
    }

    pub(crate) fn snapshot(&self) -> Vec<OpenedBuffer> {
        let mut shown: Vec<OpenedBuffer> = self
            .open
            .iter()
            .map(|(buffer, open)| open.shown(*buffer))
            .collect();
        shown.sort_by_key(|opened| opened.buffer);
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place() -> Place {
        (
            EnvironmentName::new("this-computer".to_owned()),
            PathBuf::from("/home/dev/app/src/main.rs"),
        )
    }

    #[test]
    fn two_opens_of_one_file_share_a_single_read_and_buffer() {
        let mut buffers = Buffers::default();
        let (environment, path) = place();
        assert!(buffers.begin_open(&environment, &path, RequestId(1)));
        assert!(!buffers.begin_open(&environment, &path, RequestId(2)));
        let mut outbox = Vec::new();
        assert_eq!(
            buffers.read(place(), Ok("fn main() {}\n".to_owned()), &mut outbox),
            Some(BufferId(1))
        );
        assert!(
            matches!(outbox.as_slice(), [EventKind::BufferOpened(opened)] if opened.buffer == BufferId(1))
        );
        assert!(buffers.shown(&environment, &path).is_some());
        assert_eq!(buffers.snapshot().len(), 1);
    }

    #[test]
    fn a_failed_read_tells_every_waiting_request() {
        let mut buffers = Buffers::default();
        let (environment, path) = place();
        buffers.begin_open(&environment, &path, RequestId(1));
        buffers.begin_open(&environment, &path, RequestId(2));
        let mut outbox = Vec::new();
        assert_eq!(
            buffers.read(place(), Err("not text".to_owned()), &mut outbox),
            None
        );
        assert_eq!(outbox.len(), 2);
        assert_eq!(buffers.snapshot(), Vec::new());
    }

    #[test]
    fn edits_advance_the_version_and_a_stale_base_is_refused() {
        let mut buffers = Buffers::default();
        let (environment, path) = place();
        buffers.begin_open(&environment, &path, RequestId(1));
        let mut outbox = Vec::new();
        assert!(buffers
            .read(place(), Ok("ab".to_owned()), &mut outbox)
            .is_some());
        let buffer = BufferId(1);
        let insert = TextEdit::new(1, 1, "X");
        let reply = buffers.edit(
            buffer,
            (BufferVersion(0), &insert),
            RequestId(5),
            &mut outbox,
        );
        assert_eq!(reply, Reply::Accepted);
        let late = buffers.edit(
            buffer,
            (BufferVersion(0), &insert),
            RequestId(6),
            &mut outbox,
        );
        assert_eq!(late, Reply::Accepted);
        assert_eq!(
            buffers.saving(buffer).map(|saving| saving.text),
            Some("aXXb".to_owned())
        );
        let ahead = buffers.edit(
            buffer,
            (BufferVersion(9), &insert),
            RequestId(7),
            &mut outbox,
        );
        assert!(matches!(
            ahead,
            Reply::Refused {
                code: ErrorCode::Invalid,
                ..
            }
        ));
        buffers.saved(buffer, BufferVersion(2), &mut outbox);
        assert_eq!(buffers.snapshot()[0].saved, BufferVersion(2));
        assert_eq!(buffers.close(buffer, &mut outbox), Reply::Accepted);
        assert!(buffers.shown(&environment, &path).is_none());
    }
}
