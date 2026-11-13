mod generation;
mod rows;
mod sheet;

pub(super) use generation::LoadGeneration;

use gpui_kit::{Context, Window};
use krad_domain::{EnvironmentName, HunkFingerprint, SessionChanges, SessionId};
use krad_protocol::{
    ChangesAct, ChangesCommand, ChangesRead, Command, RequestId, SessionChangesRead,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rows::ReviewedFile;

use super::EnvironmentShell;

#[derive(Default)]
pub(super) struct SessionChangesUi {
    review: Option<SessionReview>,
}

impl SessionChangesUi {
    pub(super) const fn sheet_shown(&self) -> bool {
        self.review.is_some()
    }
}

struct SessionReview {
    env: EnvironmentName,
    session: SessionId,
    load: ReviewLoad,
    awaiting: Option<RequestId>,
    expanded: HashSet<PathBuf>,
}

enum ReviewLoad {
    Reading,
    NoRepository,
    Failed(String),
    Ready { files: Vec<ReviewedFile> },
}

fn close(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.session_changes.review = None;
    cx.notify();
}

impl EnvironmentShell {
    fn current_session_id(&self, env: &EnvironmentName) -> Option<SessionId> {
        self.envs
            .get(env)?
            .data
            .current_session()
            .map(|session| session.id)
    }

    fn session_changes_of(
        &self,
        env: &EnvironmentName,
        session: SessionId,
    ) -> Option<&SessionChanges> {
        self.envs.get(env)?.data.session(session)?.changes.as_ref()
    }

    pub(super) fn capture_session_baseline(&mut self, _: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(session) = self.current_session_id(&env) else {
            return;
        };
        if self.session_changes_of(&env, session).is_some() {
            return;
        }
        self.runtime.client.post(Command::Changes(ChangesCommand {
            environment: env,
            session,
            act: ChangesAct::Capture,
        }));
    }

    pub(super) fn open_session_changes(&mut self, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(session) = self.current_session_id(&env) else {
            return;
        };
        self.session_changes.review = Some(SessionReview {
            env,
            session,
            load: ReviewLoad::Reading,
            awaiting: None,
            expanded: HashSet::new(),
        });
        self.load_session_changes(ChangesAct::Read, cx);
    }

    pub(super) fn reload_session_changes(&mut self, cx: &mut Context<Self>) {
        if self.session_changes.review.is_some() {
            self.load_session_changes(ChangesAct::Read, cx);
        }
    }

    fn restart_session_changes(&mut self, cx: &mut Context<Self>) {
        self.load_session_changes(ChangesAct::Restart, cx);
    }

    pub(super) fn session_changes_command(&self, act: ChangesAct) -> Option<Command> {
        let review = self.session_changes.review.as_ref()?;
        Some(Command::Changes(ChangesCommand {
            environment: review.env.clone(),
            session: review.session,
            act,
        }))
    }

    fn load_session_changes(&mut self, act: ChangesAct, cx: &mut Context<Self>) {
        let Some(command) = self.session_changes_command(act) else {
            return;
        };
        let request = self.runtime.client.next_request();
        if let Some(review) = self.session_changes.review.as_mut() {
            review.load = ReviewLoad::Reading;
            review.awaiting = Some(request);
        }
        drop(self.runtime.client.send_as(request, command));
        cx.notify();
    }

    pub(super) fn session_changes_read(&mut self, read: SessionChangesRead) {
        let Some(review) = self.session_changes.review.as_mut() else {
            return;
        };
        let current = review.env == read.environment
            && review.session == read.session
            && review.awaiting == Some(read.request);
        if !current {
            return;
        }
        review.awaiting = None;
        review.load = match read.read {
            ChangesRead::NoRepository => ReviewLoad::NoRepository,
            ChangesRead::Failed { message } => ReviewLoad::Failed(message),
            ChangesRead::Ready { files, .. } => ReviewLoad::Ready {
                files: files.into_iter().map(ReviewedFile::from).collect(),
            },
            ChangesRead::Unknown => ReviewLoad::Reading,
        };
    }

    fn toggle_session_reviewed(&mut self, prints: &[HunkFingerprint], cx: &mut Context<Self>) {
        let Some(review) = self.session_changes.review.as_ref() else {
            return;
        };
        let (env, session) = (review.env.clone(), review.session);
        let Some(changes) = self.session_changes_of(&env, session) else {
            return;
        };
        let hunks = prints.to_vec();
        let act = if changes.is_reviewed(prints) {
            ChangesAct::Unaccept { hunks }
        } else {
            ChangesAct::Accept { hunks }
        };
        self.runtime.client.post(Command::Changes(ChangesCommand {
            environment: env,
            session,
            act,
        }));
        cx.notify();
    }

    fn toggle_session_file(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(review) = self.session_changes.review.as_mut() else {
            return;
        };
        if !review.expanded.remove(path) {
            review.expanded.insert(path.to_path_buf());
        }
        cx.notify();
    }
}
