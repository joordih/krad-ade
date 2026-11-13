use super::{environment_at, start, Folder, TestClock, TestResult, WAIT};
use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{AgentPreset, Kraddie, Settings};
use krad_protocol::{Command, EventKind, Feed, KraddieCommand, Reply};
use krad_runtime::KraddiesSetup;
use krad_store::KraddieShelf;
use std::time::Instant;

#[test]
fn a_kraddie_the_runtime_cannot_save_is_told_to_its_clients() -> TestResult {
    let folder = Folder::new("kraddie-unsaved")?;
    let blocked = folder.0.join("blocked");
    std::fs::write(&blocked, "not a folder")?;
    let setup = KraddiesSetup {
        shelf: Some(KraddieShelf::at(blocked)),
        ordinary: None,
        clock: TestClock::new().clock(),
    };
    let runtime = start(
        environment_at(&folder.0, &[])?,
        Settings::default(),
        (setup, None),
    )?;
    let mut events = runtime.subscribe();
    let kraddie = Kraddie::new("Night Owl".parse()?, AgentPreset::ClaudeCode);
    let save = Command::Kraddie(KraddieCommand::Save {
        kraddie: Box::new(kraddie),
        replaced: None,
    });
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), save))?,
        Reply::Accepted
    );
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        match block_on(events.next()) {
            Some(Feed::Event(event)) => {
                if let EventKind::KraddieNotSaved { error } = event.kind {
                    assert_ne!(error, "");
                    return Ok(());
                }
            }
            Some(Feed::Snapshot(_)) => {}
            None => break,
        }
    }
    Err("no client heard that the Kraddie was not saved".into())
}
