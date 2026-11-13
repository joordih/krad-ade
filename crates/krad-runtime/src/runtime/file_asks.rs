use krad_protocol::{ErrorCode, FileAsk, FileCommand, Reply, RequestId};

use super::{refused, Runtime};
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::file_asks::{self, upload};
use crate::jobs::JobDone;

const UNKNOWN_UPLOAD: &str = "no upload is running with this request";

impl Runtime {
    pub(super) fn execute_file(&mut self, id: RequestId, command: FileCommand) -> Reply {
        if let FileAsk::CancelUpload { upload } = command.ask {
            return if self.uploads.cancel(upload) {
                Reply::Accepted
            } else {
                refused(ErrorCode::NotFound, UNKNOWN_UPLOAD)
            };
        }
        let Some(host) = self.environments.host_of(&command.environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let link = self.hosts.link(&host);
        let FileAsk::Upload {
            dir,
            sources,
            existing,
        } = command.ask
        else {
            let ask = command.ask;
            return self.answer_in_job(id, move || file_asks::answer(&link, ask));
        };
        let progress = self.uploads.start(id);
        let started = self.jobs.run(move || JobDone::Uploaded {
            request: id,
            answer: Box::new(upload(&link, (&dir, &sources), existing, &progress)),
        });
        match started {
            Ok(()) => Reply::Accepted,
            Err(error) => {
                self.uploads.finished(id, &mut Vec::new());
                refused(ErrorCode::Unavailable, &error.to_string())
            }
        }
    }
}
