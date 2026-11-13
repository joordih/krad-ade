#![cfg(unix)]

#[path = "support/repo.rs"]
mod repo;

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    CloudRunner, CloudRunnerId, CloudSettings, EnvironmentData, EnvironmentName, GroupId,
    OfflineWebhooks, RunnerCli, RunnerLogin, RuntimeState, SessionSpec, Settings,
};
use krad_host::{new_install_id, HostContext, APP_DIR, HOST_CLI_BUILD, HOST_CLI_PROGRAM};
use krad_protocol::{
    CloudCommand, CloudNews, CloudOpened, Command, EventKind, Feed, Reply, RunnerCommand,
    RunnerLogins, Scope,
};
use krad_runtime::{EnvironmentsSetup, Runtime, RuntimeHandle, RuntimeSetup, SettingsSetup};
use krad_store::Secrets;
use repo::{committed_repo, git, Repo};
use std::error::Error;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;

const WAIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(50);
const TIMED_OUT: &str = "the awaited event never came";
const FEED_ENDED: &str = "the feed ended";
const FAKE_SSH_DIR: &str = "tests/fixtures/fake-ssh";
const PATH_VAR: &str = "PATH";
const CONFIG_HOME_VAR: &str = "XDG_CONFIG_HOME";
const RUNNER_HOME_VAR: &str = "KRAD_FAKE_RUNNER_HOME";
const RUNNER_HOST: &str = "ready.invalid";
const ENVIRONMENT: &str = "main";
const RUNNER: CloudRunnerId = CloudRunnerId(1);
const PAIRING: &str = "00112233445566778899aabbccddeeff";
const BRANCH_PREFIX: &str = "krad/";
const EXECUTABLE: u32 = 0o755;
const BIN_DIR: &str = "bin";
const PAIR_VERB: &str = "cloud-pair";

struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn temp_folder(name: &str) -> Result<Folder, Box<dyn Error>> {
    let path = std::env::temp_dir().join(format!("krad-runtime-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&path)?;
    Ok(Folder(path.canonicalize()?))
}

fn install_fake_cli(runner_home: &Path) -> Result<(), Box<dyn Error>> {
    let bin = runner_home.join(APP_DIR).join(HOST_CLI_BUILD).join(BIN_DIR);
    std::fs::create_dir_all(&bin)?;
    let cli = bin.join(HOST_CLI_PROGRAM);
    let script = format!(
        "#!/bin/sh\ncase \"$1\" in\n  --version) echo '{HOST_CLI_BUILD}' ;;\n  {PAIR_VERB}) echo '{PAIRING}' ;;\n  *) exit 0 ;;\nesac\n"
    );
    std::fs::write(&cli, script)?;
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(EXECUTABLE))?;
    Ok(())
}

fn use_fake_runner(config: &Path, runner_home: &Path) {
    let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join(FAKE_SSH_DIR);
    let mut search = vec![fake];
    search.extend(
        std::env::var_os(PATH_VAR)
            .iter()
            .flat_map(std::env::split_paths),
    );
    if let Ok(path) = std::env::join_paths(search) {
        std::env::set_var(PATH_VAR, path);
    }
    std::env::set_var(CONFIG_HOME_VAR, config);
    std::env::set_var(RUNNER_HOME_VAR, runner_home);
}

fn start(root: &Path) -> Result<(RuntimeHandle, GroupId, Receiver<Feed>), Box<dyn Error>> {
    let mut data = EnvironmentData::empty(EnvironmentName::new(ENVIRONMENT.to_owned()));
    let group = data.add_group(root.to_path_buf(), None);
    data.enter_sessions();
    data.new_session(Some(group), SessionSpec::default());
    let mut cloud = CloudSettings::default();
    cloud.runners.push(CloudRunner {
        id: RUNNER,
        name: RUNNER_HOST.parse()?,
        destination: RUNNER_HOST.parse()?,
        offline_webhooks: OfflineWebhooks::default(),
    });
    let runtime = Runtime::start(RuntimeSetup {
        environments: EnvironmentsSetup {
            state: RuntimeState {
                environments: vec![data],
                ..RuntimeState::default()
            },
            file: None,
        },
        settings: SettingsSetup {
            settings: Settings {
                cloud,
                ..Settings::default()
            },
            file: None,
        },
        secrets: Secrets::in_memory(),
        ..RuntimeSetup::default()
    })?;
    runtime.use_host_context(HostContext {
        install_id: new_install_id(),
        ..HostContext::default()
    });
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

fn until<T>(
    feed: &Receiver<Feed>,
    mut wanted: impl FnMut(EventKind) -> Option<T>,
) -> Result<T, Box<dyn Error>> {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        match feed.recv_timeout(POLL) {
            Ok(Feed::Event(event)) => {
                if let Some(found) = wanted(event.kind) {
                    return Ok(found);
                }
            }
            Ok(Feed::Snapshot(_)) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(FEED_ENDED.into()),
        }
    }
    Err(TIMED_OUT.into())
}

fn news(
    feed: &Receiver<Feed>,
    wanted: impl Fn(&CloudNews) -> bool,
) -> Result<CloudNews, Box<dyn Error>> {
    until(feed, |kind| match kind {
        EventKind::CloudNews(news) if wanted(&news) => Some(news),
        EventKind::CloudNews(
            failed @ (CloudNews::NotPrepared { .. }
            | CloudNews::ResultFailed { .. }
            | CloudNews::RunnerFailed { .. }),
        ) => Some(failed),
        _ => None,
    })
}

fn send(runtime: &RuntimeHandle, command: Command) -> Result<Reply, Box<dyn Error>> {
    Ok(block_on(runtime.send(runtime.next_request(), command))?)
}

fn cloned_with_remote(origin: &Repo, name: &str) -> Result<Folder, Box<dyn Error>> {
    let parent = temp_folder(name)?;
    let clone = parent.0.join("work");
    let status = std::process::Command::new("git")
        .args(["clone", "--quiet"])
        .arg(&origin.0)
        .arg(&clone)
        .status()?;
    if !status.success() {
        return Err("the local clone failed".into());
    }
    Ok(parent)
}

fn opened_with_settled_logins(
    feed: &Receiver<Feed>,
) -> Result<(RunnerLogins, CloudOpened), Box<dyn Error>> {
    let mut settled: Option<RunnerLogins> = None;
    let mut opened: Option<CloudOpened> = None;
    until(feed, |kind| {
        match kind {
            EventKind::RunnerLoginsChanged(logins)
                if logins.logins.len() == RunnerCli::ALL.len()
                    && logins
                        .logins
                        .iter()
                        .all(|known| known.login != RunnerLogin::Checking) =>
            {
                settled = Some(logins);
            }
            EventKind::CloudSessionOpened(session) => opened = Some(session),
            _ => {}
        }
        (settled.is_some() && opened.is_some()).then_some(())
    })?;
    match (settled, opened) {
        (Some(logins), Some(opened)) => Ok((logins, opened)),
        _ => Err(TIMED_OUT.into()),
    }
}

#[test]
fn a_cloud_session_is_prepared_on_a_ready_runner_and_its_branch_comes_home() -> TestResult {
    let config = temp_folder("cloud-config")?;
    let runner_home = temp_folder("cloud-runner-home")?;
    use_fake_runner(&config.0, &runner_home.0);
    let origin = committed_repo("cloud-origin")?;
    git(
        &origin.0,
        &["config", "receive.denyCurrentBranch", "ignore"],
    )?;
    let local = cloned_with_remote(&origin, "cloud-local")?;
    let root = local.0.join("work");
    install_fake_cli(&runner_home.0)?;
    let (runtime, group, feed) = start(&root)?;
    let environment = EnvironmentName::new(ENVIRONMENT.to_owned());

    let create = CloudCommand::Create {
        environment: environment.clone(),
        group,
        runner: RUNNER,
    };
    assert_eq!(send(&runtime, Command::Cloud(create))?, Reply::Accepted);
    assert!(matches!(
        news(&feed, |news| matches!(news, CloudNews::Preparing { .. }))?,
        CloudNews::Preparing { runner: RUNNER }
    ));
    let (logins, opened) = opened_with_settled_logins(&feed)?;
    assert_eq!(logins.runner, RUNNER);
    let login_of = |cli: RunnerCli| {
        logins
            .logins
            .iter()
            .find(|known| known.cli == cli)
            .map(|known| known.login)
    };
    assert_eq!(login_of(RunnerCli::Claude), Some(RunnerLogin::SignedOut));
    assert_eq!(login_of(RunnerCli::Github), Some(RunnerLogin::SignedIn));
    assert_eq!(login_of(RunnerCli::Gitlab), Some(RunnerLogin::SignedOut));
    assert!(runtime.runner(RUNNER).is_some());
    assert_eq!(opened.group, group);
    assert_eq!(opened.starts.len(), 1);
    let data = block_on(runtime.snapshot(Scope::Runtime))?
        .environments
        .into_iter()
        .next()
        .ok_or("the runtime lost the environment")?;
    let placement = data
        .session(opened.session)
        .and_then(|session| session.cloud.clone())
        .ok_or("the opened session is not placed on the runner")?;
    assert_eq!(placement.runner, RUNNER);
    assert!(placement.branch.starts_with(BRANCH_PREFIX));
    assert!(placement.worktree.starts_with(&runner_home.0));
    assert!(placement.worktree.join(repo::TRACKED).is_file());

    let push = CloudCommand::Push {
        environment: environment.clone(),
        session: opened.session,
    };
    assert_eq!(send(&runtime, Command::Cloud(push))?, Reply::Accepted);
    let pushed = news(&feed, |news| matches!(news, CloudNews::Pushed { .. }))?;
    assert_eq!(
        pushed,
        CloudNews::Pushed {
            branch: placement.branch.clone()
        }
    );
    git(
        &origin.0,
        &["rev-parse", "--verify", "--quiet", &placement.branch],
    )?;

    let home = CloudCommand::BringHome {
        environment,
        session: opened.session,
    };
    assert_eq!(send(&runtime, Command::Cloud(home))?, Reply::Accepted);
    let brought = news(&feed, |news| {
        matches!(news, CloudNews::BroughtHome { .. } | CloudNews::NoRepo)
    })?;
    assert_eq!(
        brought,
        CloudNews::BroughtHome {
            branch: placement.branch.clone()
        }
    );
    let groups = block_on(runtime.snapshot(Scope::Runtime))?
        .environments
        .into_iter()
        .next()
        .map_or(0, |data| data.groups.len());
    assert_eq!(groups, 2);

    assert_eq!(
        send(
            &runtime,
            Command::Runner(RunnerCommand::Remove { runner: RUNNER })
        )?,
        Reply::Accepted
    );
    Ok(())
}
