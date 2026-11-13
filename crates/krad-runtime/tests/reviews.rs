#![cfg(unix)]

#[path = "support/code_host.rs"]
mod code_host;
#[path = "support/repo.rs"]
mod repo;

use code_host::{FakeCodeHost, Route};
use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::review::{CiStatus, MergeRequestIid, MergeRequestQuery, ProjectPath};
use krad_domain::{
    AppEvent, CodeHost, EnvironmentData, EnvironmentName, GitlabSettings, GitlabTokens,
    ReviewVerdict, RuntimeState, SessionSpec, Settings,
};
use krad_protocol::{
    Command, ErrorCode, EventKind, Feed, Fetched, Reply, RequestRef, ReviewAct, ReviewAnswer,
    ReviewAnswered, ReviewCommand, ReviewConnection, ReviewWrite,
};
use krad_runtime::{EnvironmentsSetup, Runtime, RuntimeHandle, RuntimeSetup, SettingsSetup};
use krad_store::Secrets;
use repo::{committed_repo, git};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const CONFIG_HOME_VAR: &str = "XDG_CONFIG_HOME";
const WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);
const TIMED_OUT: &str = "the awaited event never came";
const PROJECT: &str = "team/app";
const IID: MergeRequestIid = MergeRequestIid(3);

const USER: &str = r#"{"id":7,"username":"ana","name":"Ana"}"#;
const MERGE_REQUEST: &str = r#"{"iid":3,"title":"Fix the durations","state":"opened","author":{"id":7,"username":"ana","name":"Ana"},"source_branch":"fix-durations","target_branch":"main","web_url":"http://code.test/team/app/-/merge_requests/3","created_at":"2026-11-01T10:00:00Z","updated_at":"2026-11-02T10:00:00Z","head_pipeline":{"id":40,"sha":"abc1234","ref":"fix-durations","status":"failed","web_url":"http://code.test/p/40"}}"#;
const MERGE_REQUESTS: &str = concat!(
    "[",
    r#"{"iid":3,"title":"Fix the durations","state":"opened","author":{"id":7,"username":"ana","name":"Ana"},"source_branch":"fix-durations","target_branch":"main","web_url":"http://code.test/team/app/-/merge_requests/3","created_at":"2026-11-01T10:00:00Z","updated_at":"2026-11-02T10:00:00Z"}"#,
    "]"
);
const APPROVALS: &str = r#"{"approved":false,"approvals_required":2,"approvals_left":1,"approved_by":[{"user":{"id":8,"username":"bo","name":"Bo"}}]}"#;

struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn routes() -> Vec<Route> {
    let get = |path, body| Route {
        method: "GET",
        path,
        body,
    };
    vec![
        get("/api/v4/user", USER),
        get("/api/v4/projects/team%2Fapp/merge_requests", MERGE_REQUESTS),
        get(
            "/api/v4/projects/team%2Fapp/merge_requests/3",
            MERGE_REQUEST,
        ),
        get(
            "/api/v4/projects/team%2Fapp/merge_requests/3/approvals",
            APPROVALS,
        ),
        Route {
            method: "POST",
            path: "/api/v4/projects/team%2Fapp/merge_requests/3/approve",
            body: "{}",
        },
    ]
}

fn kept_login(host: &str) -> Result<Secrets, Box<dyn Error>> {
    let secrets = Secrets::in_memory();
    let tokens = GitlabTokens {
        access_token: "fixture-access".to_owned(),
        refresh_token: "fixture-refresh".to_owned(),
    };
    secrets.write_gitlab_login(host, &tokens)?;
    Ok(secrets)
}

fn start(
    root: &Path,
    gitlab_host: &str,
    secrets: Secrets,
) -> Result<(RuntimeHandle, Receiver<Feed>), Box<dyn Error>> {
    let mut data = EnvironmentData::empty(EnvironmentName::new(ENVIRONMENT.to_owned()));
    let group = data.add_group(root.to_path_buf(), None);
    data.enter_sessions();
    data.new_session(Some(group), SessionSpec::default());
    let settings = Settings {
        gitlab: GitlabSettings {
            host: gitlab_host.to_owned(),
            client_id: "app".to_owned(),
            ..GitlabSettings::default()
        },
        ..Settings::default()
    };
    let runtime = Runtime::start(RuntimeSetup {
        environments: EnvironmentsSetup {
            state: RuntimeState {
                environments: vec![data],
                ..RuntimeState::default()
            },
            file: None,
        },
        settings: SettingsSetup {
            settings,
            file: None,
        },
        secrets,
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
    Ok((runtime, received))
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
            Err(RecvTimeoutError::Disconnected) => return Err("the feed ended".into()),
        }
    }
    Err(TIMED_OUT.into())
}

fn ask(
    runtime: &RuntimeHandle,
    feed: &Receiver<Feed>,
    root: &Path,
    act: ReviewAct,
) -> Result<ReviewAnswer, Box<dyn Error>> {
    let request = runtime.next_request();
    let command = Command::Review(ReviewCommand {
        environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
        root: root.to_path_buf(),
        act,
    });
    assert_eq!(block_on(runtime.send(request, command))?, Reply::Accepted);
    let answered = until(
        feed,
        |event| matches!(event, EventKind::ReviewAnswered(answered) if answered.request == request),
    )?;
    let EventKind::ReviewAnswered(answered) = answered else {
        return Err(TIMED_OUT.into());
    };
    let ReviewAnswered { answer, .. } = *answered;
    Ok(answer)
}

fn approve_and_hear_it(
    runtime: &RuntimeHandle,
    feed: &Receiver<Feed>,
    root: &Path,
    project: ProjectPath,
) -> TestResult {
    let approve = ReviewAct::Write {
        request: RequestRef { project, iid: IID },
        write: ReviewWrite::Approve,
    };
    assert_eq!(
        ask(runtime, feed, root, approve)?,
        ReviewAnswer::Written { failure: None }
    );
    let sent = until(feed, |event| {
        let EventKind::AppEvent(record) = event else {
            return false;
        };
        matches!(record.event, AppEvent::ReviewSent { .. })
    })?;
    let EventKind::AppEvent(record) = sent else {
        return Err(TIMED_OUT.into());
    };
    assert_eq!(
        record.event,
        AppEvent::ReviewSent {
            merge_request: IID,
            verdict: ReviewVerdict::Approved,
        }
    );

    Ok(())
}

#[test]
fn a_gitlab_review_connects_lists_reads_facts_and_approves_through_the_runtime() -> TestResult {
    let server = FakeCodeHost::serve(routes())?;
    let config = Folder(std::env::temp_dir().join(format!(
        "krad-runtime-reviews-config-{}",
        std::process::id()
    )));
    std::env::set_var(CONFIG_HOME_VAR, &config.0);
    let secrets = kept_login(&server.url)?;
    let repo = committed_repo("reviews")?;
    let remote = format!("{}/{PROJECT}.git", server.url);
    git(&repo.0, &["remote", "add", "origin", &remote])?;
    let (runtime, feed) = start(&repo.0, &server.url, secrets)?;

    let refused = block_on(runtime.send(
        runtime.next_request(),
        Command::Review(ReviewCommand {
            environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
            root: repo.0.clone(),
            act: ReviewAct::List {
                query: MergeRequestQuery::default(),
            },
        }),
    ))?;
    assert!(matches!(
        refused,
        Reply::Refused {
            code: ErrorCode::Unavailable,
            ..
        }
    ));

    let ReviewAnswer::Connected { connection } = ask(&runtime, &feed, &repo.0, ReviewAct::Connect)?
    else {
        return Err("not a connection".into());
    };
    let ReviewConnection::Ready { host, me } = connection else {
        return Err(format!("not connected: {connection:?}").into());
    };
    assert_eq!((host, me.username.as_str()), (CodeHost::Gitlab, "ana"));

    let query = MergeRequestQuery::default();
    let ReviewAnswer::Listed {
        listed: Fetched::Ready(listed),
    } = ask(&runtime, &feed, &repo.0, ReviewAct::List { query })?
    else {
        return Err("the list was not read".into());
    };
    assert_eq!(listed.project.to_string(), PROJECT);
    assert_eq!(
        listed
            .requests
            .iter()
            .map(|request| request.iid)
            .collect::<Vec<_>>(),
        [IID]
    );

    let facts = ReviewAct::Facts {
        project: listed.project.clone(),
        iids: vec![IID],
    };
    let ReviewAnswer::Facts {
        iid,
        facts: Fetched::Ready(facts),
    } = ask(&runtime, &feed, &repo.0, facts)?
    else {
        return Err("the facts were not read".into());
    };
    assert_eq!(iid, IID);
    assert_eq!(facts.pipeline, Some(CiStatus::Failed));
    assert_eq!(facts.approvals.approvals_left, 1);

    approve_and_hear_it(&runtime, &feed, &repo.0, listed.project)?;
    assert!(server
        .seen()
        .iter()
        .all(|line| !line.contains("fixture-access")));

    let elsewhere = Folder(std::env::temp_dir().join(format!(
        "krad-runtime-reviews-elsewhere-{}",
        std::process::id()
    )));
    std::fs::create_dir_all(&elsewhere.0)?;
    let ReviewAnswer::Issues {
        issues: Fetched::Failed(reason),
    } = ask(&runtime, &feed, &elsewhere.0, ReviewAct::Issues)?
    else {
        return Err("issues outside a project were read".into());
    };
    assert!(reason.contains("does not point at a GitLab or GitHub project"));
    Ok(())
}
