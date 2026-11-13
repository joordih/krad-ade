use krad_domain::{
    EnvironmentName, FilePatch, HunkFingerprint, SessionChanges, SessionEdit, SessionId,
};
use krad_protocol::{
    ChangesAct, ChangesCommand, ChangesRead, ErrorCode, EventKind, Reply, RequestId,
    SessionChangesRead,
};
use std::collections::BTreeSet;
use std::path::PathBuf;

use super::{refused, Runtime, UNSUPPORTED_COMMAND};
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::git::{
    capture_baseline, read_session, Act, ActOrder, Asker, AskpassPlan, ChangesDone, GitJob,
    SessionRead,
};
use crate::jobs::JobDone;

const UNKNOWN_SESSION: &str = "this environment has no session of that id";
const NOT_A_REPO: &str = "The session's group is not inside a git repository";
const NOT_STARTED: &str = "Could not start reading the session's changes";
const NO_CHANGES: &str = "the session has no captured changes";
const ALREADY_REVIEWED: &str = "these hunks are already marked that way";

fn present_prints(files: &[FilePatch]) -> BTreeSet<HunkFingerprint> {
    files.iter().flat_map(FilePatch::fingerprints).collect()
}

impl Runtime {
    pub(super) fn execute_changes(&mut self, request: RequestId, command: ChangesCommand) -> Reply {
        let ChangesCommand {
            environment,
            session,
            act,
        } = command;
        let Some(data) = self.environments.data(&environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let Some(found) = data.session(session) else {
            return refused(ErrorCode::NotFound, UNKNOWN_SESSION);
        };
        let baseline = found
            .changes
            .as_ref()
            .map(|changes| changes.baseline.clone());
        let host = data.host.clone();
        let root = found
            .group
            .and_then(|group| data.group(group))
            .map(|group| group.root.clone());
        let group = found.group;
        match act {
            ChangesAct::Capture => {
                if baseline.is_some() {
                    return Reply::Accepted;
                }
                let Some(root) = root else {
                    return Reply::Accepted;
                };
                let link = self.hosts.link(&host);
                self.start_changes_job(move || ChangesDone::Captured {
                    environment,
                    session,
                    baseline: capture_baseline(&link, &root),
                })
            }
            ChangesAct::Read => self.read_changes(request, environment, session, root, baseline),
            ChangesAct::Restart => {
                let cleared = SessionEdit::SetChanges {
                    session,
                    changes: None,
                };
                self.environments.edit(
                    environment.clone(),
                    &cleared.into(),
                    None,
                    &mut self.outbox,
                );
                self.read_changes(request, environment, session, root, None)
            }
            ChangesAct::Revert { patch, hunk } => {
                let Some(repo) = root
                    .as_deref()
                    .and_then(|root| self.git.at(&host, root))
                    .map(|git| git.repo_root.clone())
                else {
                    return refused(ErrorCode::NotFound, NOT_A_REPO);
                };
                let order = ActOrder {
                    asker: Asker::Client(request),
                    environment,
                    group,
                    link: self.hosts.link(&host),
                    repo,
                    askpass: AskpassPlan::Off,
                    act: Act::RevertSession { patch, hunk },
                };
                self.start_act(order)
            }
            ChangesAct::Accept { hunks } => {
                self.mark_reviewed(environment, session, (&hunks, true))
            }
            ChangesAct::Unaccept { hunks } => {
                self.mark_reviewed(environment, session, (&hunks, false))
            }
            ChangesAct::Unknown => refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND),
        }
    }

    fn mark_reviewed(
        &mut self,
        environment: EnvironmentName,
        session: SessionId,
        (hunks, reviewed): (&[HunkFingerprint], bool),
    ) -> Reply {
        let changes = self
            .environments
            .data(&environment)
            .and_then(|data| data.session(session))
            .and_then(|found| found.changes.clone());
        let Some(mut changes) = changes else {
            return refused(ErrorCode::NotFound, NO_CHANGES);
        };
        let already = if reviewed {
            changes.is_reviewed(hunks)
        } else {
            hunks.iter().all(|hunk| !changes.reviewed.contains(hunk))
        };
        if already || hunks.is_empty() {
            return refused(ErrorCode::AlreadyAnswered, ALREADY_REVIEWED);
        }
        changes.set_reviewed(hunks, reviewed);
        let edit = SessionEdit::SetChanges {
            session,
            changes: Some(changes),
        };
        self.environments
            .edit(environment, &edit.into(), None, &mut self.outbox);
        Reply::Accepted
    }

    fn read_changes(
        &mut self,
        request: RequestId,
        environment: EnvironmentName,
        session: SessionId,
        root: Option<PathBuf>,
        baseline: Option<krad_domain::GitObjectId>,
    ) -> Reply {
        let Some(root) = root else {
            self.outbox
                .push(EventKind::SessionChangesRead(SessionChangesRead {
                    request,
                    environment,
                    session,
                    read: ChangesRead::NoRepository,
                }));
            return Reply::Accepted;
        };
        let Some(host) = self.environments.host_of(&environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let link = self.hosts.link(&host);
        self.start_changes_job(move || ChangesDone::Read {
            request,
            environment,
            session,
            read: read_session(&link, &root, baseline),
        })
    }

    fn start_changes_job(&self, work: impl FnOnce() -> ChangesDone + Send + 'static) -> Reply {
        match self.jobs.run(move || JobDone::Git(GitJob::Changes(work()))) {
            Ok(()) => Reply::Accepted,
            Err(error) => {
                tracing::warn!(%error, "could not start a session changes job");
                refused(ErrorCode::Unsupported, NOT_STARTED)
            }
        }
    }

    pub(super) fn changes_done(&mut self, done: ChangesDone) {
        match done {
            ChangesDone::Captured {
                environment,
                session,
                baseline,
            } => match baseline {
                Ok(Some(baseline)) => self.edit_changes(&environment, session, |changes| {
                    changes.get_or_insert_with(|| SessionChanges::new(baseline));
                }),
                Ok(None) => {}
                Err(error) => tracing::debug!(%error, "session baseline not recorded"),
            },
            ChangesDone::Read {
                request,
                environment,
                session,
                read,
            } => {
                let read = match read {
                    Ok(None) => ChangesRead::NoRepository,
                    Err(error) => ChangesRead::Failed {
                        message: error.to_string(),
                    },
                    Ok(Some(SessionRead {
                        repo,
                        baseline,
                        files,
                    })) => {
                        let present = present_prints(&files);
                        self.edit_changes(&environment, session, |changes| {
                            changes
                                .get_or_insert_with(|| SessionChanges::new(baseline))
                                .keep_reviewed_of(&present);
                        });
                        ChangesRead::Ready { repo, files }
                    }
                };
                self.outbox
                    .push(EventKind::SessionChangesRead(SessionChangesRead {
                        request,
                        environment,
                        session,
                        read,
                    }));
            }
        }
    }

    fn edit_changes(
        &mut self,
        environment: &EnvironmentName,
        session: SessionId,
        edit: impl FnOnce(&mut Option<SessionChanges>),
    ) {
        let Some(before) = self
            .environments
            .data(environment)
            .and_then(|data| data.session(session))
            .map(|found| found.changes.clone())
        else {
            return;
        };
        let mut changes = before.clone();
        edit(&mut changes);
        if changes == before {
            return;
        }
        let set = SessionEdit::SetChanges { session, changes };
        self.environments
            .edit(environment.clone(), &set.into(), None, &mut self.outbox);
    }
}
