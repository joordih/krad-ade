#![cfg(unix)]

#[path = "kraddies/chats.rs"]
mod chats;
#[path = "kraddies/digests.rs"]
mod digests;
#[path = "kraddies/memory.rs"]
mod memory;
#[path = "kraddies/preamble.rs"]
mod preamble;
#[path = "support/repo.rs"]
mod repo;
#[path = "kraddies/runs.rs"]
mod runs;
#[path = "kraddies/saving.rs"]
mod saving;

use futures::executor::block_on;
use krad_chat::new_session_id;
use krad_domain::{
    AgentPreset, EnvironmentData, EnvironmentName, LocalMoment, RuntimeState, Settings,
};
use krad_protocol::{KraddiesSnapshot, Scope};
use krad_runtime::{
    BusSetup, Clock, EnvironmentsSetup, KraddiesSetup, Runtime, RuntimeHandle, RuntimeSetup,
    SettingsSetup,
};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const BULLETS_HARNESS: &str = "tests/fixtures/fake-harness/claude-bullets";
const WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);
const TIMED_OUT: &str = "the awaited state never came";

struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Result<Self, Box<dyn Error>> {
        let path = std::env::temp_dir().join(format!("krad-{name}-{}", new_session_id().as_str()));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path.canonicalize()?))
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

#[derive(Clone)]
struct TestClock {
    base: SystemTime,
    ahead: Arc<AtomicU64>,
}

impl TestClock {
    fn new() -> Self {
        Self {
            base: SystemTime::now(),
            ahead: Arc::default(),
        }
    }

    fn advance(&self, by: Duration) {
        self.ahead.fetch_add(by.as_secs(), Ordering::SeqCst);
    }

    fn clock(&self) -> Clock {
        let (base, ahead) = (self.base, Arc::clone(&self.ahead));
        Clock::new(move || {
            LocalMoment::utc(base + Duration::from_secs(ahead.load(Ordering::SeqCst)))
        })
    }
}

fn harness(path: &str) -> Result<String, Box<dyn Error>> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(path)
        .canonicalize()?
        .to_string_lossy()
        .into_owned())
}

fn claude_at(program: &str) -> Settings {
    let mut settings = Settings::default();
    settings
        .harness_commands
        .insert(AgentPreset::ClaudeCode, program.to_owned());
    settings
}

fn environment_at(root: &Path, chats: &[u64]) -> Result<EnvironmentData, Box<dyn Error>> {
    let mut data = EnvironmentData::empty(EnvironmentName::new(ENVIRONMENT.to_owned()));
    let group = data.add_group(root.to_path_buf(), None);
    data.nodes = chats
        .iter()
        .map(|id| {
            serde_json::from_value(serde_json::json!({
                "id": id, "title": "chat", "state": "Idle", "kind": "Chat", "group": group.0
            }))
        })
        .collect::<Result<_, _>>()?;
    data.next_id = chats.iter().max().map_or(1, |last| last + 1);
    data.enter_sessions();
    Ok(data)
}

fn start(
    data: EnvironmentData,
    settings: Settings,
    (kraddies, bus): (KraddiesSetup, Option<BusSetup>),
) -> Result<RuntimeHandle, Box<dyn Error>> {
    Ok(Runtime::start(RuntimeSetup {
        bus,
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
        kraddies,
        ..RuntimeSetup::default()
    })?)
}

fn kraddies_of(runtime: &RuntimeHandle) -> Result<KraddiesSnapshot, Box<dyn Error>> {
    Ok(*block_on(runtime.snapshot(Scope::Runtime))?.kraddies)
}

fn environment_of(runtime: &RuntimeHandle) -> Result<EnvironmentData, Box<dyn Error>> {
    block_on(runtime.snapshot(Scope::Runtime))?
        .environments
        .into_iter()
        .next()
        .ok_or_else(|| "the runtime lost the environment".into())
}

fn wait_for<T>(
    mut found: impl FnMut() -> Result<Option<T>, Box<dyn Error>>,
) -> Result<T, Box<dyn Error>> {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Some(value) = found()? {
            return Ok(value);
        }
        std::thread::sleep(POLL);
    }
    Err(TIMED_OUT.into())
}
