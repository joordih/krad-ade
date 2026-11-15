use krad_protocol::{ErrorCode, FileAsk, FileCommand, Reply, RequestId};
use std::time::Instant;

use super::{refused, Runtime};
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::file_asks::{self, upload};
use crate::folder_watch::Folder;
use crate::jobs::JobDone;

const UNKNOWN_UPLOAD: &str = "no upload is running with this request";

impl Runtime {
    pub(super) fn execute_file(&mut self, id: RequestId, command: FileCommand) -> Reply {
        match command.ask {
            FileAsk::Watch { dir } => {
                return self
                    .folders
                    .watch((command.environment, dir), Instant::now());
            }
            FileAsk::Unwatch { dir } => return self.folders.unwatch(&(command.environment, dir)),
            _ => {}
        }
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

impl Runtime {
    pub(super) fn tick_folders(&mut self) {
        for folder in self.folders.due(Instant::now()) {
            let Some(host) = self.environments.host_of(&folder.0) else {
                self.folders
                    .listed(folder, Ok(Vec::new()), Instant::now(), &mut self.outbox);
                continue;
            };
            let link = self.hosts.link(&host);
            let watched = folder.clone();
            let started = self.jobs.run(move || {
                let listed =
                    krad_repo::list_dir(&link, &watched.1).map_err(|error| error.to_string());
                JobDone::FolderListed {
                    folder: watched,
                    listed,
                }
            });
            if let Err(error) = started {
                tracing::debug!(%error, "a watched folder was not listed");
                self.folders
                    .listed(folder, Ok(Vec::new()), Instant::now(), &mut self.outbox);
            }
        }
    }

    pub(super) fn folder_listed(
        &mut self,
        (folder, listed): (Folder, Result<Vec<krad_domain::DirEntry>, String>),
    ) {
        self.folders
            .listed(folder, listed, Instant::now(), &mut self.outbox);
    }
}
