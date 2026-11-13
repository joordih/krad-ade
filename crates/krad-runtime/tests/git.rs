#![cfg(unix)]

#[path = "support/repo.rs"]
mod repo;

#[path = "git/bus.rs"]
mod bus;

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    AppEvent, ChangeKind, CommitMode, EnvironmentData, EnvironmentName, GroupGit, GroupId,
    RuntimeState, SessionId, SessionSpec,
};
use krad_protocol::{
    ChangesAct, ChangesCommand, ChangesRead, Command, ErrorCode, EventKind, Feed, GitAct,
    GitCommand, GitOutcome, Reply, RequestId, Scope,
};
use krad_runtime::{EnvironmentsSetup, Runtime, RuntimeHandle, RuntimeSetup};
use repo::{committed_repo, Repo, FIRST_TEXT, TRACKED};
use std::error::Error;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);
const TIMED_OUT: &str = "the awaited event never came";
const FEED_ENDED: &str = "the feed ended";
const NO_GROUP: &str = "the environment has no group";
const UNTRACKED: &str = "scratch.txt";
const CHANGED_TEXT: &str = "first\nsecond\n";

fn main_name() -> EnvironmentName {
    EnvironmentName::new(ENVIRONMENT.to_owned())
}

fn start(root: &Path) -> Result<(RuntimeHandle, GroupId, Receiver<Feed>), Box<dyn Error>> {
    let mut data = EnvironmentData::empty(main_name());
    let group = data.add_group(root.to_path_buf(), None);
    data.enter_sessions();
    data.new_session(Some(group), SessionSpec::default());
    let runtime = Runtime::start(RuntimeSetup {
        environments: EnvironmentsSetup {
            state: RuntimeState {
                environments: vec![data],
                ..RuntimeState::default()
            },
            file: None,
        },
        ..RuntimeSetup::default()
    })?;
    let mut feed = runtime.subscribe();
    let (heard, received) = channel();
    std::thread::spawn(move || {
        while let Some(next) = block_on(feed.next()) {
            if heard.send(next).is_err() {
                break;
            }
        }
    });
    Ok((runtime, group, received))
}

fn until(
    feed: &Receiver<Feed>,
    mut wanted: impl FnMut(&EventKind) -> bool,
) -> Result<EventKind, Box<dyn Error>> {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        match feed.recv_timeout(POLL) {
            Ok(Feed::Event(event)) if wanted(&event.kind) => return Ok(event.kind),
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(FEED_ENDED.into()),
        }
    }
    Err(TIMED_OUT.into())
}

fn read_at(
    feed: &Receiver<Feed>,
    root: &Path,
    mut wanted: impl FnMut(&GroupGit) -> bool,
) -> Result<GroupGit, Box<dyn Error>> {
    match until(feed, |kind| {
        matches!(kind, EventKind::GitRead(read)
            if read.root == root && read.git.as_deref().is_some_and(&mut wanted))
    })? {
        EventKind::GitRead(read) => read.git.map(|git| *git).ok_or_else(|| TIMED_OUT.into()),
        _ => Err(TIMED_OUT.into()),
    }
}

fn act(
    runtime: &RuntimeHandle,
    group: GroupId,
    act: GitAct,
) -> Result<(RequestId, Reply), Box<dyn Error>> {
    let request = runtime.next_request();
    let command = Command::Git(GitCommand {
        environment: main_name(),
        group,
        act,
    });
    Ok((request, block_on(runtime.send(request, command))?))
}

fn finished(feed: &Receiver<Feed>, request: RequestId) -> Result<GitOutcome, Box<dyn Error>> {
    match until(
        feed,
        |kind| matches!(kind, EventKind::GitFinished(done) if done.request == request),
    )? {
        EventKind::GitFinished(done) => Ok(done.outcome),
        _ => Err(TIMED_OUT.into()),
    }
}

fn acted(
    runtime: &RuntimeHandle,
    feed: &Receiver<Feed>,
    group: GroupId,
    git: GitAct,
) -> Result<GitOutcome, Box<dyn Error>> {
    let (request, reply) = act(runtime, group, git)?;
    assert_eq!(reply, Reply::Accepted);
    finished(feed, request)
}

fn environment(runtime: &RuntimeHandle) -> Result<EnvironmentData, Box<dyn Error>> {
    block_on(runtime.snapshot(Scope::Runtime))?
        .environments
        .into_iter()
        .next()
        .ok_or_else(|| "the runtime lost the environment".into())
}

#[test]
fn the_runtime_reads_a_groups_git_and_stages_commits_and_refuses_on_it() -> TestResult {
    let repo = committed_repo("git-acts")?;
    std::fs::write(repo.0.join(UNTRACKED), FIRST_TEXT)?;
    let (runtime, group, feed) = start(&repo.0)?;
    let read = read_at(&feed, &repo.0, |git| !git.status.changes.is_empty())?;
    assert_eq!(read.repo_root, repo.0);
    assert_eq!(read.status.branch.as_deref(), Some("main"));
    assert!(read
        .status
        .changes
        .iter()
        .any(|change| change.kind == ChangeKind::Untracked));
    let (_, empty) = act(
        &runtime,
        group,
        GitAct::Commit {
            message: " ".to_owned(),
            mode: CommitMode::New,
            push: false,
        },
    )?;
    assert!(matches!(
        empty,
        Reply::Refused {
            code: ErrorCode::Invalid,
            ..
        }
    ));
    assert_eq!(
        acted(&runtime, &feed, group, GitAct::StageAll)?,
        GitOutcome::Done
    );
    read_at(&feed, &repo.0, |git| !git.status.staged.is_empty())?;
    let committed = acted(
        &runtime,
        &feed,
        group,
        GitAct::Commit {
            message: "Add the scratch".to_owned(),
            mode: CommitMode::New,
            push: false,
        },
    )?;
    assert_eq!(committed, GitOutcome::Done);
    read_at(&feed, &repo.0, |git| git.status.changes.is_empty())?;
    until(&feed, |kind| {
        matches!(kind, EventKind::AppEvent(record)
            if record.group == Some(group) && record.event == AppEvent::GitChanged)
    })?;
    let pushed = acted(&runtime, &feed, group, GitAct::Push)?;
    assert!(matches!(pushed, GitOutcome::Failed { .. }), "{pushed:?}");
    Ok(())
}

#[test]
fn the_runtime_makes_a_worktree_group_and_removes_it() -> TestResult {
    let repo: Repo = committed_repo("git-worktrees")?;
    let (runtime, group, feed) = start(&repo.0)?;
    read_at(&feed, &repo.0, |_| true)?;
    let (request, reply) = act(
        &runtime,
        group,
        GitAct::CreateWorktree {
            name: "try it".to_owned(),
        },
    )?;
    assert_eq!(reply, Reply::Accepted);
    until(&feed, |kind| {
        matches!(kind, EventKind::AppEvent(record)
            if record.event == AppEvent::WorktreeCreated { from: group })
    })?;
    let made = finished(&feed, request)?;
    let GitOutcome::Opened { group: opened } = made else {
        return Err(format!("no worktree group opened: {made:?}").into());
    };
    let data = environment(&runtime)?;
    let worktree = data.group(opened).ok_or(NO_GROUP)?.clone();
    let origin = worktree
        .worktree
        .clone()
        .ok_or("the group is no worktree")?;
    assert_eq!(origin.repo_root, repo.0);
    assert_eq!(origin.branch, "try-it");
    assert!(worktree.root.join(TRACKED).is_file());
    let (request, reply) = act(&runtime, opened, GitAct::RemoveWorktree)?;
    assert_eq!(reply, Reply::Accepted);
    until(&feed, |kind| {
        matches!(kind, EventKind::AppEvent(record)
            if record.group == Some(opened) && record.event == AppEvent::WorktreeRemoved)
    })?;
    assert_eq!(finished(&feed, request)?, GitOutcome::Done);
    assert!(environment(&runtime)?.group(opened).is_none());
    assert!(!worktree.root.exists());
    let (_, not_a_worktree) = act(&runtime, group, GitAct::RemoveWorktree)?;
    assert!(matches!(
        not_a_worktree,
        Reply::Refused {
            code: ErrorCode::Invalid,
            ..
        }
    ));
    Ok(())
}

fn changes(
    runtime: &RuntimeHandle,
    session: SessionId,
    act: ChangesAct,
) -> Result<(RequestId, Reply), Box<dyn Error>> {
    let request = runtime.next_request();
    let command = Command::Changes(ChangesCommand {
        environment: main_name(),
        session,
        act,
    });
    Ok((request, block_on(runtime.send(request, command))?))
}

#[test]
fn the_runtime_keeps_a_sessions_baseline_reads_its_changes_and_reverts_them() -> TestResult {
    let repo = committed_repo("git-session")?;
    let (runtime, _, feed) = start(&repo.0)?;
    let session = environment(&runtime)?
        .sessions
        .first()
        .map(|session| session.id)
        .ok_or("the environment kept no session")?;
    changes(&runtime, session, ChangesAct::Capture)?;
    let deadline = Instant::now() + WAIT;
    while environment(&runtime)?
        .session(session)
        .and_then(|found| found.changes.as_ref())
        .is_none()
    {
        if Instant::now() > deadline {
            return Err(TIMED_OUT.into());
        }
        std::thread::sleep(POLL);
    }
    std::fs::write(repo.0.join(TRACKED), CHANGED_TEXT)?;
    read_at(&feed, &repo.0, |git| !git.status.changes.is_empty())?;
    let (request, _) = changes(&runtime, session, ChangesAct::Read)?;
    let read = match until(
        &feed,
        |kind| matches!(kind, EventKind::SessionChangesRead(read) if read.request == request),
    )? {
        EventKind::SessionChangesRead(read) => read.read,
        _ => return Err(TIMED_OUT.into()),
    };
    let ChangesRead::Ready { files, .. } = read else {
        return Err(format!("the session's changes were not read: {read:?}").into());
    };
    let [patch] = files.as_slice() else {
        return Err(format!("one changed file was expected: {files:?}").into());
    };
    assert_eq!(patch.path, Path::new(TRACKED));
    let hunks = patch.fingerprints();
    let accept = || ChangesAct::Accept {
        hunks: hunks.clone(),
    };
    assert_eq!(changes(&runtime, session, accept())?.1, Reply::Accepted);
    let reviewed = environment(&runtime)?
        .session(session)
        .and_then(|found| found.changes.clone())
        .ok_or("the session lost its changes")?;
    assert!(reviewed.is_reviewed(&hunks));
    assert!(matches!(
        changes(&runtime, session, accept())?.1,
        Reply::Refused {
            code: ErrorCode::AlreadyAnswered,
            ..
        }
    ));
    let unaccept = ChangesAct::Unaccept {
        hunks: hunks.clone(),
    };
    assert_eq!(changes(&runtime, session, unaccept)?.1, Reply::Accepted);
    let (request, reply) = changes(
        &runtime,
        session,
        ChangesAct::Revert {
            patch: Box::new(patch.clone()),
            hunk: None,
        },
    )?;
    assert_eq!(reply, Reply::Accepted);
    assert_eq!(finished(&feed, request)?, GitOutcome::Done);
    assert_eq!(std::fs::read_to_string(repo.0.join(TRACKED))?, FIRST_TEXT);
    Ok(())
}
