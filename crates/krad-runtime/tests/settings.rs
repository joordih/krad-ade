use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{AppearanceMode, Settings};
use krad_protocol::{Command, EventKind, Feed, Reply, RuntimeCommand, SettingsCommand};
use krad_runtime::{EventReceiver, Runtime, RuntimeHandle, RuntimeSetup, SettingsSetup};
use krad_store::settings_file_in;
use std::error::Error;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn Error>>;

const FEED_ENDED: &str = "the feed ended";
const NOT_A_SNAPSHOT: &str = "the feed did not start from a snapshot";

struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "krad-runtime-settings-{name}-{}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&dir));
        Self(dir)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn start(settings: Settings, file: PathBuf) -> Result<RuntimeHandle, Box<dyn Error>> {
    Ok(Runtime::start(RuntimeSetup {
        settings: SettingsSetup {
            settings,
            file: Some(file),
        },
        ..RuntimeSetup::default()
    })?)
}

fn subscribed(runtime: &RuntimeHandle) -> Result<(EventReceiver, Settings), Box<dyn Error>> {
    let mut events = runtime.subscribe();
    match block_on(events.next()).ok_or(FEED_ENDED)? {
        Feed::Snapshot(snapshot) => Ok((events, *snapshot.settings)),
        Feed::Event(_) => Err(NOT_A_SNAPSHOT.into()),
    }
}

fn save(runtime: &RuntimeHandle, settings: &Settings) -> Result<Reply, Box<dyn Error>> {
    let settings = Box::new(settings.clone());
    Ok(block_on(runtime.send(
        runtime.next_request(),
        Command::Settings(SettingsCommand::Save { settings }),
    ))?)
}

fn events_until_pong(
    runtime: &RuntimeHandle,
    events: &mut EventReceiver,
) -> Result<Vec<EventKind>, Box<dyn Error>> {
    let marker = runtime.next_request();
    drop(block_on(
        runtime.send(marker, Command::Runtime(RuntimeCommand::Ping)),
    )?);
    let mut heard = Vec::new();
    loop {
        match block_on(events.next()).ok_or(FEED_ENDED)? {
            Feed::Event(event) => match event.kind {
                EventKind::Pong { request } if request == marker => return Ok(heard),
                kind => heard.push(kind),
            },
            Feed::Snapshot(_) => {}
        }
    }
}

fn read(path: &Path) -> Result<Settings, Box<dyn Error>> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn dark() -> Settings {
    Settings {
        appearance: AppearanceMode::Dark,
        ..Settings::default()
    }
}

#[test]
fn the_runtime_writes_saved_settings_and_tells_every_client() -> TestResult {
    let folder = Folder::new("save");
    let file = settings_file_in(&folder.0);
    let runtime = start(Settings::default(), file.clone())?;
    let (mut events, shown) = subscribed(&runtime)?;
    assert_eq!(shown, Settings::default());
    assert_eq!(save(&runtime, &dark())?, Reply::Accepted);
    assert_eq!(save(&runtime, &dark())?, Reply::Accepted);
    assert_eq!(
        events_until_pong(&runtime, &mut events)?,
        [EventKind::SettingsChanged {
            settings: Box::new(dark()),
        }]
    );
    block_on(runtime.settle())?;
    assert_eq!(read(&file)?, dark());
    let (_, shown) = subscribed(&runtime)?;
    assert_eq!(shown, dark());
    drop(runtime);
    let restarted = start(read(&file)?, file)?;
    assert_eq!(subscribed(&restarted)?.1, dark());
    Ok(())
}

#[test]
fn a_settings_file_that_cannot_be_written_is_told() -> TestResult {
    let folder = Folder::new("unwritable");
    std::fs::create_dir_all(&folder.0)?;
    let blocker = folder.0.join("not-a-folder");
    std::fs::write(&blocker, b"")?;
    let runtime = start(Settings::default(), settings_file_in(&blocker))?;
    let (mut events, _) = subscribed(&runtime)?;
    save(&runtime, &dark())?;
    block_on(runtime.settle())?;
    let heard = events_until_pong(&runtime, &mut events)?;
    assert!(heard
        .iter()
        .any(|kind| matches!(kind, EventKind::SettingsNotSaved { .. })));
    Ok(())
}
