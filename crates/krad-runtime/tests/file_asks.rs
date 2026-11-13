#![cfg(unix)]

#[path = "support/repo.rs"]
mod repo;

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    EntryKind, EnvironmentData, EnvironmentName, ProjectQuery, RuntimeState, UndoOutcome,
    UploadExisting,
};
use krad_protocol::{
    Answer, Command, ErrorCode, EventKind, Feed, FileAsk, FileCommand, OverLimit, Reply, RequestId,
    Unreadable, MAX_WRITE_BYTES,
};
use krad_runtime::{EnvironmentsSetup, EventReceiver, Runtime, RuntimeHandle, RuntimeSetup};
use repo::{committed_repo, FIRST_TEXT, TRACKED};
use std::error::Error;
use std::fs;
use std::path::Path;

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const WRITTEN_TEXT: &str = "written\n";
const SMALL_LIMIT: u64 = 2;
const SEARCH_LIMIT: usize = 100;

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

fn send(runtime: &RuntimeHandle, ask: FileAsk) -> Result<(RequestId, Reply), Box<dyn Error>> {
    let request = runtime.next_request();
    let command = Command::File(FileCommand {
        environment: main_name(),
        ask,
    });
    Ok((request, block_on(runtime.send(request, command))?))
}

fn answered(
    feed: &mut EventReceiver,
    request: RequestId,
    progress: &mut Vec<(u64, u64)>,
) -> Result<Answer, Box<dyn Error>> {
    while let Some(feed) = block_on(feed.next()) {
        let Feed::Event(event) = feed else {
            continue;
        };
        match event.kind {
            EventKind::UploadProgress(shown) if shown.upload == request => {
                progress.push((shown.sent, shown.total));
            }
            EventKind::Answered {
                request: heard,
                answer,
            } if heard == request => return Ok(*answer),
            _ => {}
        }
    }
    Err("the runtime stopped".into())
}

fn ask(
    runtime: &RuntimeHandle,
    feed: &mut EventReceiver,
    ask: FileAsk,
) -> Result<Answer, Box<dyn Error>> {
    let (request, reply) = send(runtime, ask)?;
    if reply != Reply::Accepted {
        return Err(format!("{reply:?}").into());
    }
    answered(feed, request, &mut Vec::new())
}

#[test]
fn a_file_is_listed_read_written_renamed_and_deleted_through_the_runtime() -> TestResult {
    let repo = committed_repo("file-asks-basics")?;
    let (runtime, mut feed) = start(&repo.0)?;
    let tracked = repo.0.join(TRACKED);
    let Answer::Entries { entries } = ask(
        &runtime,
        &mut feed,
        FileAsk::List {
            dir: repo.0.clone(),
        },
    )?
    else {
        return Err("no entries".into());
    };
    assert_eq!(
        entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.kind))
            .collect::<Vec<_>>(),
        [(TRACKED, EntryKind::File)]
    );
    let read = ask(
        &runtime,
        &mut feed,
        FileAsk::Read {
            path: tracked.clone(),
            from: 0,
            limit: SMALL_LIMIT * SMALL_LIMIT * SMALL_LIMIT,
            over: OverLimit::Refuse,
        },
    )?;
    assert!(
        matches!(read, Answer::FileText { ref text, truncated: false, modified: Some(_), .. } if text == FIRST_TEXT)
    );
    let written = ask(
        &runtime,
        &mut feed,
        FileAsk::Write {
            path: tracked.clone(),
            text: WRITTEN_TEXT.to_owned(),
        },
    )?;
    assert!(matches!(written, Answer::Written { stat: Some(stat) } if stat.len == 8));
    assert_eq!(fs::read_to_string(&tracked)?, WRITTEN_TEXT);
    let Answer::Placed { path: renamed } = ask(
        &runtime,
        &mut feed,
        FileAsk::Rename {
            path: tracked.clone(),
            name: "renamed.txt".to_owned(),
        },
    )?
    else {
        return Err("not renamed".into());
    };
    assert_eq!(renamed, repo.0.join("renamed.txt"));
    let stat = ask(&runtime, &mut feed, FileAsk::Stat { path: tracked })?;
    assert_eq!(stat, Answer::Stat { stat: None });
    let deleted = ask(
        &runtime,
        &mut feed,
        FileAsk::Delete {
            path: renamed.clone(),
        },
    )?;
    assert_eq!(deleted, Answer::Done);
    assert!(!renamed.exists());
    Ok(())
}

#[test]
fn reads_and_writes_past_their_bounds_are_refused_with_a_reason() -> TestResult {
    let repo = committed_repo("file-asks-bounds")?;
    let (runtime, mut feed) = start(&repo.0)?;
    let tracked = repo.0.join(TRACKED);
    let refused = ask(
        &runtime,
        &mut feed,
        FileAsk::Read {
            path: tracked.clone(),
            from: 0,
            limit: SMALL_LIMIT,
            over: OverLimit::Refuse,
        },
    )?;
    assert_eq!(
        refused,
        Answer::Unreadable {
            unreadable: Unreadable::TooLarge {
                len: 6,
                limit: SMALL_LIMIT
            }
        }
    );
    let chunk = ask(
        &runtime,
        &mut feed,
        FileAsk::Read {
            path: tracked.clone(),
            from: SMALL_LIMIT,
            limit: SMALL_LIMIT,
            over: OverLimit::Truncate,
        },
    )?;
    assert!(
        matches!(chunk, Answer::FileText { ref text, len: 6, truncated: true, .. } if text == "rs")
    );
    let too_large = ask(
        &runtime,
        &mut feed,
        FileAsk::Write {
            path: tracked.clone(),
            text: "x".repeat(MAX_WRITE_BYTES + 1),
        },
    )?;
    assert!(matches!(
        too_large,
        Answer::Failed {
            code: ErrorCode::Invalid,
            ..
        }
    ));
    assert_eq!(fs::read_to_string(&tracked)?, FIRST_TEXT);
    let (_, unknown) = send(
        &runtime,
        FileAsk::CancelUpload {
            upload: RequestId(u64::MAX),
        },
    )?;
    assert!(matches!(
        unknown,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    Ok(())
}

#[test]
fn an_upload_reports_its_progress_and_keeps_what_already_exists_when_asked() -> TestResult {
    let repo = committed_repo("file-asks-upload")?;
    let (runtime, mut feed) = start(&repo.0)?;
    let source = repo.0.join("source");
    fs::create_dir_all(source.join("inner"))?;
    fs::write(source.join("inner/data.bin"), [7_u8; 4096])?;
    let target = repo.0.join("target");
    fs::create_dir_all(target.join("source/inner"))?;
    fs::write(target.join("source/inner/data.bin"), "kept")?;
    fs::write(source.join("fresh.txt"), FIRST_TEXT)?;
    let taken = ask(
        &runtime,
        &mut feed,
        FileAsk::Taken {
            dir: target.clone(),
            names: vec!["source".to_owned(), "missing".to_owned()],
        },
    )?;
    assert_eq!(
        taken,
        Answer::Taken {
            names: vec!["source".to_owned()]
        }
    );
    let (request, reply) = send(
        &runtime,
        FileAsk::Upload {
            dir: target.clone(),
            sources: vec![source],
            existing: UploadExisting::Skip,
        },
    )?;
    assert_eq!(reply, Reply::Accepted);
    let mut progress = Vec::new();
    assert_eq!(answered(&mut feed, request, &mut progress)?, Answer::Done);
    assert_eq!(progress.last(), Some(&(4096 + 6, 4096 + 6)));
    assert_eq!(
        fs::read_to_string(target.join("source/inner/data.bin"))?,
        "kept"
    );
    assert_eq!(
        fs::read_to_string(target.join("source/fresh.txt"))?,
        FIRST_TEXT
    );
    Ok(())
}

#[test]
fn a_project_search_is_replaced_and_the_replacement_undone() -> TestResult {
    let repo = committed_repo("file-asks-search")?;
    let (runtime, mut feed) = start(&repo.0)?;
    let tracked = repo.0.join(TRACKED);
    let Answer::ProjectMatches { matches } = ask(
        &runtime,
        &mut feed,
        FileAsk::Search {
            root: repo.0.clone(),
            query: ProjectQuery {
                pattern: "fir".to_owned(),
                ..ProjectQuery::default()
            },
            limit: SEARCH_LIMIT,
        },
    )?
    else {
        return Err("no matches".into());
    };
    assert_eq!(matches.match_count(), 1);
    let Answer::Replaced { files } = ask(
        &runtime,
        &mut feed,
        FileAsk::Replace {
            files: matches.files,
            replacement: "thi".to_owned(),
        },
    )?
    else {
        return Err("not replaced".into());
    };
    assert_eq!(fs::read_to_string(&tracked)?, "thist\n");
    let undos = files
        .into_iter()
        .filter_map(|file| file.ok()?.undo)
        .collect::<Vec<_>>();
    assert_eq!(undos.len(), 1);
    let undone = ask(&runtime, &mut feed, FileAsk::UndoReplace { undos })?;
    assert_eq!(
        undone,
        Answer::Undone {
            outcomes: vec![Ok(UndoOutcome::Restored)]
        }
    );
    assert_eq!(fs::read_to_string(&tracked)?, FIRST_TEXT);
    Ok(())
}
