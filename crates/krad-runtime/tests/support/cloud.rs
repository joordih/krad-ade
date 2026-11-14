use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    AgentPreset, CloudRunner, CloudRunnerId, CloudSettings, EnvironmentData, EnvironmentName,
    GroupId, InstallId, OfflineWebhooks, RunnerCli, RunnerLogin, RuntimeState, SessionSpec,
    Settings,
};
use krad_host::new_install_id;
use krad_protocol::{
    CloudCommand, CloudNews, CloudOpened, CloudReach, Command, EventKind, Feed, Reply,
};
use krad_protocol::{RunnerLogins, Scope};
use krad_runtime::{
    EnvironmentsSetup, HostTools, Runtime, RuntimeHandle, RuntimeSetup, SettingsSetup,
};
use krad_store::Secrets;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::repo::Repo;

pub type TestResult = Result<(), Box<dyn Error>>;

const WAIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(50);
const TIMED_OUT: &str = "the awaited event never came";
const FEED_ENDED: &str = "the feed ended";
const NO_INSTALL: &str = "no installation id could be made";
const NO_ENVIRONMENT: &str = "the runtime lost the environment";
const FAKE_SSH_DIR: &str = "tests/fixtures/fake-ssh";
const PATH_VAR: &str = "PATH";
const CONFIG_HOME_VAR: &str = "XDG_CONFIG_HOME";
const RUNNER_HOME_VAR: &str = "KRAD_FAKE_RUNNER_HOME";
const RUNNER_HOST: &str = "ready.invalid";
const FOLDER_PREFIX: &str = "kr";
pub const ENVIRONMENT: &str = "main";
pub const RUNNER: CloudRunnerId = CloudRunnerId(1);

pub struct Folder(pub PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

pub fn temp_folder(name: &str) -> Result<Folder, Box<dyn Error>> {
    let path = std::env::temp_dir().join(format!("{FOLDER_PREFIX}-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&path)?;
    Ok(Folder(path.canonicalize()?))
}

pub fn use_fake_runner(config: &Path, runner_home: &Path) {
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

pub fn environment() -> EnvironmentName {
    EnvironmentName::new(ENVIRONMENT.to_owned())
}

pub struct Started {
    pub runtime: RuntimeHandle,
    pub group: GroupId,
    pub feed: Receiver<Feed>,
}

pub fn install_id() -> Result<InstallId, Box<dyn Error>> {
    Ok(new_install_id().ok_or(NO_INSTALL)?)
}

pub fn start(root: &Path, install: InstallId) -> Result<Started, Box<dyn Error>> {
    let mut data = EnvironmentData::empty(environment());
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
                default_harness: AgentPreset::ClaudeCode,
                ..Settings::default()
            },
            file: None,
        },
        secrets: Secrets::in_memory(),
        host_tools: HostTools {
            install_id: Some(install),
            ..HostTools::default()
        },
        ..RuntimeSetup::default()
    })?;
    let mut events = runtime.subscribe();
    let (heard, feed) = channel();
    std::thread::spawn(move || {
        while let Some(next) = block_on(events.next()) {
            if heard.send(next).is_err() {
                break;
            }
        }
    });
    Ok(Started {
        runtime,
        group,
        feed,
    })
}

pub fn until<T>(
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

pub fn news(
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

pub fn send(runtime: &RuntimeHandle, command: Command) -> Result<Reply, Box<dyn Error>> {
    Ok(block_on(runtime.send(runtime.next_request(), command))?)
}

pub fn environment_data(runtime: &RuntimeHandle) -> Result<EnvironmentData, Box<dyn Error>> {
    block_on(runtime.snapshot(Scope::Runtime))?
        .environments
        .into_iter()
        .next()
        .ok_or_else(|| NO_ENVIRONMENT.into())
}

pub fn cloned_with_remote(origin: &Repo, name: &str) -> Result<Folder, Box<dyn Error>> {
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

pub fn create_cloud_session(
    started: &Started,
    reach: CloudReach,
    first: Option<krad_protocol::FirstChat>,
) -> Result<(RunnerLogins, CloudOpened), Box<dyn Error>> {
    let create = CloudCommand::Create {
        environment: environment(),
        group: started.group,
        runner: RUNNER,
        reach,
        first,
    };
    if send(&started.runtime, Command::Cloud(create))? != Reply::Accepted {
        return Err("the cloud session was refused".into());
    }
    match news(&started.feed, |news| {
        matches!(news, CloudNews::Preparing { .. })
    })? {
        CloudNews::Preparing { runner: RUNNER } => Ok(()),
        other => Err(format!("the cloud session did not prepare: {other:?}")),
    }?;
    opened_with_settled_logins(&started.feed)
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
