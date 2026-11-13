#![cfg(unix)]

#[path = "support/repo.rs"]
mod repo;

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    ChangeKind, EnvironmentData, EnvironmentName, FileChange, RuntimeState, WorktreeBase,
    WorktreeState,
};
use krad_protocol::{
    Answer, Command, DiffOf, ErrorCode, EventKind, Feed, Reply, RepoAsk, RepoCommand,
    WorktreeChecked, WorktreeProbe, WorktreeRemoval,
};
use krad_runtime::{EnvironmentsSetup, EventReceiver, Runtime, RuntimeHandle, RuntimeSetup};
use repo::{committed_repo, FIRST_TEXT, TRACKED};
use std::error::Error;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const CHANGED_TEXT: &str = "first\nsecond\n";
const REPLACED_TEXT: &str = "replaced\n";
const WORKTREE: &str = "repo-asks-fix";
const HISTORY_LIMIT: usize = 10;

fn main_name() -> EnvironmentName {
    EnvironmentName::new(ENVIRONMENT.to_owned())
}

fn start(root: &Path) -> Result<(RuntimeHandle, EventReceiver), Box<dyn Error>> {
    let mut data = EnvironmentData::empty(main_name());
    data.add_group(root.to_path_buf(), None);
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
    block_on(feed.next()).ok_or("no snapshot")?;
    Ok((runtime, feed))
}

fn ask(
    runtime: &RuntimeHandle,
    feed: &mut EventReceiver,
    ask: RepoAsk,
) -> Result<Answer, Box<dyn Error>> {
    let request = runtime.next_request();
    let command = Command::Repo(RepoCommand {
        environment: main_name(),
        ask,
    });
    let reply = block_on(runtime.send(request, command))?;
    if reply != Reply::Accepted {
        return Err(format!("{reply:?}").into());
    }
    while let Some(feed) = block_on(feed.next()) {
        let Feed::Event(event) = feed else {
            continue;
        };
        if let EventKind::Answered {
            request: heard,
            answer,
        } = event.kind
        {
            if heard == request {
                return Ok(*answer);
            }
        }
    }
    Err("the runtime stopped".into())
}

fn tracked_change() -> FileChange {
    FileChange {
        path: PathBuf::from(TRACKED),
        kind: ChangeKind::Modified,
        origin: None,
    }
}

#[test]
fn an_unknown_environment_is_refused_before_any_work() -> TestResult {
    let repo = committed_repo("repo-asks-unknown")?;
    let (runtime, _feed) = start(&repo.0)?;
    let command = Command::Repo(RepoCommand {
        environment: EnvironmentName::new("elsewhere".to_owned()),
        ask: RepoAsk::Tasks {
            root: repo.0.clone(),
        },
    });
    let reply = block_on(runtime.send(runtime.next_request(), command))?;
    assert!(matches!(
        reply,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    Ok(())
}

#[test]
fn a_group_s_history_diff_and_text_are_read_and_replaced_through_the_runtime() -> TestResult {
    let repo = committed_repo("repo-asks-read")?;
    let (runtime, mut feed) = start(&repo.0)?;
    let commits = ask(
        &runtime,
        &mut feed,
        RepoAsk::CommitLog {
            repo: repo.0.clone(),
            revisions: vec!["HEAD".to_owned()],
            limit: HISTORY_LIMIT,
        },
    )?;
    let Answer::Commits { commits } = commits else {
        return Err(format!("{commits:?}").into());
    };
    assert_eq!(commits.len(), 1);
    std::fs::write(repo.0.join(TRACKED), CHANGED_TEXT)?;
    let diff = ask(
        &runtime,
        &mut feed,
        RepoAsk::Diff {
            repo: repo.0.clone(),
            of: DiffOf::Working {
                change: tracked_change(),
            },
        },
    )?;
    assert!(matches!(diff, Answer::Diff { ref text } if text.contains("+second")));
    let path = repo.0.join(TRACKED);
    let stale = ask(
        &runtime,
        &mut feed,
        RepoAsk::ReplaceText {
            path: path.clone(),
            expected: FIRST_TEXT.to_owned(),
            text: REPLACED_TEXT.to_owned(),
        },
    )?;
    assert_eq!(stale, Answer::TextReplaced { replaced: false });
    let replaced = ask(
        &runtime,
        &mut feed,
        RepoAsk::ReplaceText {
            path: path.clone(),
            expected: CHANGED_TEXT.to_owned(),
            text: REPLACED_TEXT.to_owned(),
        },
    )?;
    assert_eq!(replaced, Answer::TextReplaced { replaced: true });
    assert_eq!(std::fs::read_to_string(&path)?, REPLACED_TEXT);
    let missing = ask(
        &runtime,
        &mut feed,
        RepoAsk::ReadText {
            path: repo.0.join("absent.txt"),
            max_bytes: 64,
        },
    )?;
    assert!(matches!(missing, Answer::Failed { .. }));
    Ok(())
}

#[test]
fn a_worktree_is_added_checked_and_removed_through_the_runtime() -> TestResult {
    let repo = committed_repo("repo-asks-worktree")?;
    let (runtime, mut feed) = start(&repo.0)?;
    let added = ask(
        &runtime,
        &mut feed,
        RepoAsk::AddWorktree {
            repo: repo.0.clone(),
            name: WORKTREE.to_owned(),
        },
    )?;
    let Answer::WorktreeAdded { worktree } = added else {
        return Err(format!("{added:?}").into());
    };
    assert!(worktree.path.join(TRACKED).exists());
    let checked = ask(
        &runtime,
        &mut feed,
        RepoAsk::WorktreeStates {
            base: WorktreeBase::default(),
            worktrees: vec![WorktreeProbe {
                main: repo.0.clone(),
                path: worktree.path.clone(),
                branch: worktree.branch.clone(),
            }],
        },
    )?;
    assert_eq!(
        checked,
        Answer::WorktreeStates {
            states: vec![WorktreeChecked {
                path: worktree.path.clone(),
                state: Ok(WorktreeState::Fresh),
            }],
        }
    );
    let removed = ask(
        &runtime,
        &mut feed,
        RepoAsk::RemoveWorktree {
            repo: repo.0.clone(),
            path: worktree.path.clone(),
            removal: WorktreeRemoval::Clean,
        },
    )?;
    assert_eq!(removed, Answer::Done);
    assert!(!worktree.path.exists());
    Ok(())
}
