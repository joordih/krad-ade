use krad_protocol::{
    BufferId, EditorCommand, ErrorCode, EventKind, Problem, ProblemScope, Reply, RequestId,
    MAX_READ_BYTES,
};

use super::{refused, Runtime};
use crate::buffers::Saving;
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::jobs::{BufferJob, JobDone};

const NOT_TEXT: &str = "the file is not UTF-8 text";
const NO_BUFFER: &str = "no buffer is open with that id";
const UNKNOWN_VERB: &str = "this runtime does not know that editor command";

impl Runtime {
    pub(super) fn execute_editor(&mut self, id: RequestId, command: EditorCommand) -> Reply {
        match command {
            EditorCommand::Open { environment, path } => {
                if let Some(shown) = self.editor.shown(&environment, &path) {
                    self.outbox.push(shown);
                    return Reply::Accepted;
                }
                let Some(host) = self.environments.host_of(&environment) else {
                    return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
                };
                if !self.editor.begin_open(&environment, &path, id) {
                    return Reply::Accepted;
                }
                let link = self.hosts.link(&host);
                self.start_buffer_job(move || {
                    let read = link
                        .read(&path, MAX_READ_BYTES)
                        .map_err(|error| error.to_string())
                        .and_then(|bytes| {
                            String::from_utf8(bytes).map_err(|_| NOT_TEXT.to_owned())
                        });
                    BufferJob::Read {
                        place: (environment, path),
                        read,
                    }
                })
            }
            EditorCommand::Edit {
                buffer,
                based_on,
                edit,
            } => {
                let reply = self
                    .editor
                    .edit(buffer, (based_on, &edit), id, &mut self.outbox);
                if reply == Reply::Accepted {
                    self.language_buffer_edited(buffer);
                }
                reply
            }
            EditorCommand::Save { buffer } => self.save_buffer(buffer),
            EditorCommand::Close { buffer } => {
                self.language_buffer_closed(buffer);
                self.editor.close(buffer, &mut self.outbox)
            }
            EditorCommand::Unknown => refused(ErrorCode::Unsupported, UNKNOWN_VERB),
        }
    }

    fn save_buffer(&mut self, buffer: BufferId) -> Reply {
        let Some(Saving {
            buffer,
            environment,
            path,
            text,
            version,
        }) = self.editor.saving(buffer)
        else {
            return refused(ErrorCode::NotFound, NO_BUFFER);
        };
        let Some(host) = self.environments.host_of(&environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let link = self.hosts.link(&host);
        self.start_buffer_job(move || BufferJob::Written {
            buffer,
            version,
            environment,
            written: link
                .write(&path, text.as_bytes())
                .map_err(|error| error.to_string()),
        })
    }

    fn start_buffer_job(&self, work: impl FnOnce() -> BufferJob + Send + 'static) -> Reply {
        match self.jobs.run(move || JobDone::Buffer(work())) {
            Ok(()) => Reply::Accepted,
            Err(error) => refused(ErrorCode::Unavailable, &error.to_string()),
        }
    }

    pub(super) fn buffer_job_done(&mut self, done: BufferJob) {
        match done {
            BufferJob::Read { place, read } => {
                if let Some(buffer) = self.editor.read(place, read, &mut self.outbox) {
                    self.attach_language(buffer);
                }
            }
            BufferJob::Written {
                buffer,
                version,
                environment,
                written,
            } => match written {
                Ok(()) => {
                    self.editor.saved(buffer, version, &mut self.outbox);
                    self.language_buffer_saved(buffer);
                }
                Err(message) => self.outbox.push(EventKind::Problem(Problem {
                    scope: ProblemScope::Environment { environment },
                    code: ErrorCode::Failed,
                    message,
                })),
            },
        }
    }
}
