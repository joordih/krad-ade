mod generation;
mod rows;
mod sheet;

pub(super) use generation::LoadGeneration;

use gpui_kit::{AppContext as _, Context, Window};
use krad_domain::{EnvironmentName, GitObjectId, HunkFingerprint, SessionChanges, SessionId};
use krad_host::HostLink;
use krad_repo::{repo_root, session_diff, snapshot_tree, RepoError, SessionDiff};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rows::{present_prints, ReviewedFile};

use super::EnvironmentShell;

#[derive(Default)]
pub(super) struct SessionChangesUi {
    review: Option<SessionReview>,
    generation: LoadGeneration,
}

struct SessionReview {
    env: EnvironmentName,
    session: SessionId,
    load: ReviewLoad,
    expanded: HashSet<PathBuf>,
}

enum ReviewLoad {
    Reading,
    NoRepository,
    Failed(String),
    Ready {
        repo: PathBuf,
        files: Vec<ReviewedFile>,
    },
}

struct ReadSession {
    repo: PathBuf,
    baseline: GitObjectId,
    diff: SessionDiff,
}

fn read_session(
    link: &HostLink,
    root: &Path,
    baseline: Option<GitObjectId>,
) -> Result<Option<ReadSession>, RepoError> {
    let Some(repo) = repo_root(link, root)? else {
        return Ok(None);
    };
    let baseline = match baseline {
        Some(baseline) => baseline,
        None => snapshot_tree(link, &repo)?,
    };
    let diff = session_diff(link, &repo, &baseline)?;
    Ok(Some(ReadSession {
        repo,
        baseline,
        diff,
    }))
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

    fn session_root(&self, env: &EnvironmentName, session: SessionId) -> Option<PathBuf> {
        let data = &self.envs.get(env)?.data;
        let group = data.session(session)?.group?;
        data.group(group).map(|group| group.root.clone())
    }

    fn session_changes_of(
        &self,
        env: &EnvironmentName,
        session: SessionId,
    ) -> Option<&SessionChanges> {
        self.envs.get(env)?.data.session(session)?.changes.as_ref()
    }

    fn edit_session_changes(
        &mut self,
        env: &EnvironmentName,
        session: SessionId,
        edit: impl FnOnce(&mut Option<SessionChanges>),
    ) {
        let Some(session) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.data.session_mut(session))
        else {
            return;
        };
        let before = session.changes.clone();
        edit(&mut session.changes);
        if session.changes != before {
            self.store();
        }
    }

    pub(super) fn capture_session_baseline(&mut self, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(session) = self.current_session_id(&env) else {
            return;
        };
        let Some(root) = self.session_root(&env, session) else {
            return;
        };
        if self.session_changes_of(&env, session).is_some() {
            return;
        }
        let link = self.env_link(&env);
        let snapshot = cx.background_spawn(async move {
            let Some(repo) = repo_root(&link, &root)? else {
                return Ok(None);
            };
            snapshot_tree(&link, &repo).map(Some)
        });
        cx.spawn(async move |this, cx| {
            let snapshot = snapshot.await;
            this.update(cx, |shell, _| match snapshot {
                Ok(Some(baseline)) => shell.edit_session_changes(&env, session, |changes| {
                    changes.get_or_insert_with(|| SessionChanges::new(baseline));
                }),
                Ok(None) => {}
                Err(error) => {
                    tracing::debug!(%error, "session baseline not recorded");
                }
            })
            .unwrap_or_default();
        })
        .detach();
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
            expanded: HashSet::new(),
        });
        self.load_session_changes(cx);
    }

    pub(super) fn reload_session_changes(&mut self, cx: &mut Context<Self>) {
        if self.session_changes.review.is_some() {
            self.load_session_changes(cx);
        }
    }

    fn restart_session_changes(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.session_changes.review.as_ref() else {
            return;
        };
        let (env, session) = (review.env.clone(), review.session);
        self.edit_session_changes(&env, session, |changes| *changes = None);
        self.load_session_changes(cx);
    }

    fn load_session_changes(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.session_changes.review.as_mut() else {
            return;
        };
        let (env, session) = (review.env.clone(), review.session);
        review.load = ReviewLoad::Reading;
        let generation = self.session_changes.generation.advance();
        cx.notify();
        let Some(root) = self.session_root(&env, session) else {
            self.finish_session_load(&env, session, generation, Ok(None));
            return;
        };
        let baseline = self
            .session_changes_of(&env, session)
            .map(|changes| changes.baseline.clone());
        let link = self.env_link(&env);
        let reading = cx.background_spawn(async move { read_session(&link, &root, baseline) });
        cx.spawn(async move |this, cx| {
            let read = reading.await;
            this.update(cx, |shell, cx| {
                shell.finish_session_load(&env, session, generation, read);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn finish_session_load(
        &mut self,
        env: &EnvironmentName,
        session: SessionId,
        generation: LoadGeneration,
        read: Result<Option<ReadSession>, RepoError>,
    ) {
        let still_open = self
            .session_changes
            .review
            .as_ref()
            .is_some_and(|review| review.env == *env && review.session == session);
        if !still_open || !self.session_changes.generation.admits(generation) {
            return;
        }
        let load = match read {
            Ok(None) => ReviewLoad::NoRepository,
            Err(error) => ReviewLoad::Failed(error.to_string()),
            Ok(Some(ReadSession {
                repo,
                baseline,
                diff,
            })) => {
                let files: Vec<ReviewedFile> =
                    diff.files.into_iter().map(ReviewedFile::from).collect();
                let present = present_prints(&files);
                self.edit_session_changes(env, session, |changes| {
                    changes
                        .get_or_insert_with(|| SessionChanges::new(baseline))
                        .keep_reviewed_of(&present);
                });
                ReviewLoad::Ready { repo, files }
            }
        };
        if let Some(review) = self.session_changes.review.as_mut() {
            review.load = load;
        }
    }

    fn toggle_session_reviewed(&mut self, prints: &[HunkFingerprint], cx: &mut Context<Self>) {
        let Some(review) = self.session_changes.review.as_ref() else {
            return;
        };
        let (env, session) = (review.env.clone(), review.session);
        self.edit_session_changes(&env, session, |changes| {
            if let Some(changes) = changes {
                let reviewed = !changes.is_reviewed(prints);
                changes.set_reviewed(prints, reviewed);
            }
        });
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
