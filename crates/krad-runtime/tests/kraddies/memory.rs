use super::{environment_at, kraddies_of, start, Folder, TestClock, TestResult, ENVIRONMENT};
use futures::executor::block_on;
use krad_agents::{call, Caller, Request, Response, TerminalHandle};
use krad_domain::{AgentPreset, Kraddie, KraddieId, MemoryLine, Settings, Switch};
use krad_protocol::{Command, ErrorCode, KraddieCommand, Reply};
use krad_runtime::{BusSetup, KraddiesSetup, RuntimeHandle};
use krad_store::KraddieShelf;
use std::error::Error;

const FIRST: &str = "The api runs on port 8080";
const REVISED: &str = "The api runs on port 9090";
const FROM_THE_BUS: &str = "Staging deploys from the release branch";
const SOME_TERMINAL: u64 = 99;

fn send(runtime: &RuntimeHandle, command: KraddieCommand) -> Result<Reply, Box<dyn Error>> {
    Ok(block_on(
        runtime.send(runtime.next_request(), Command::Kraddie(command)),
    )?)
}

fn memory_of(runtime: &RuntimeHandle) -> Result<Vec<String>, Box<dyn Error>> {
    let kraddies = kraddies_of(runtime)?;
    let state = kraddies.kraddies.first().ok_or("no Kraddie")?;
    Ok(state.mind.memory.lines().to_vec())
}

fn remember_over_the_bus(runtime: &RuntimeHandle, kraddie: &KraddieId) -> TestResult {
    let endpoint = &runtime.bus().ok_or("no bus")?.endpoint;
    let (address, token) = (endpoint.address.to_string(), endpoint.token.clone());
    let caller = Caller::Terminal(TerminalHandle {
        environment: ENVIRONMENT.to_owned(),
        terminal: SOME_TERMINAL,
    });
    let request = Request::RememberKraddie {
        name: kraddie.as_str().to_owned(),
        text: FROM_THE_BUS.to_owned(),
    };
    let answer = std::thread::spawn(move || call(&address, &token, &caller, request))
        .join()
        .map_err(|_| "the caller panicked")??;
    if let Response::Error(error) = answer {
        return Err(format!("the bus refused to remember: {error:?}").into());
    }
    Ok(())
}

#[test]
fn a_bus_remember_and_a_keepsake_edit_both_survive() -> TestResult {
    let folder = Folder::new("kraddie-memory")?;
    let shelf = KraddieShelf::at(folder.0.clone());
    let kraddie = Kraddie::new("Night Owl".parse()?, AgentPreset::ClaudeCode);
    shelf.write(&kraddie)?;
    let setup = KraddiesSetup {
        shelf: Some(shelf),
        ordinary: None,
        clock: TestClock::new().clock(),
    };
    let data = environment_at(&folder.0, &[])?;
    let runtime = start(
        data,
        Settings::default(),
        (setup, Some(BusSetup::default())),
    )?;
    let id = kraddie.id();
    let remembered = send(
        &runtime,
        KraddieCommand::Remember {
            kraddie: id.clone(),
            text: FIRST.to_owned(),
        },
    )?;
    assert_eq!(remembered, Reply::Accepted);
    let seen = MemoryLine {
        index: 0,
        text: FIRST.to_owned(),
    };
    runtime.post(Command::Kraddie(KraddieCommand::ReviseMemory {
        kraddie: id.clone(),
        line: seen.clone(),
        text: REVISED.to_owned(),
    }));
    remember_over_the_bus(&runtime, &id)?;
    assert_eq!(memory_of(&runtime)?, [REVISED, FROM_THE_BUS]);
    let stale = send(
        &runtime,
        KraddieCommand::ForgetMemory {
            kraddie: id.clone(),
            line: seen,
        },
    )?;
    assert!(matches!(
        stale,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    let forgotten = send(
        &runtime,
        KraddieCommand::ForgetMemory {
            kraddie: id,
            line: MemoryLine {
                index: 1,
                text: FROM_THE_BUS.to_owned(),
            },
        },
    )?;
    assert_eq!(forgotten, Reply::Accepted);
    assert_eq!(memory_of(&runtime)?, [REVISED]);
    Ok(())
}

#[test]
fn a_kraddie_reads_another_s_memory_once_shared_and_never_its_own() -> TestResult {
    let folder = Folder::new("kraddie-sharing")?;
    let shelf = KraddieShelf::at(folder.0.clone());
    let reader = Kraddie::new("Night Owl".parse()?, AgentPreset::ClaudeCode);
    let from = Kraddie::new("Early Bird".parse()?, AgentPreset::ClaudeCode);
    shelf.write(&reader)?;
    shelf.write(&from)?;
    let setup = KraddiesSetup {
        shelf: Some(shelf),
        ordinary: None,
        clock: TestClock::new().clock(),
    };
    let runtime = start(
        environment_at(&folder.0, &[])?,
        Settings::default(),
        (setup, None),
    )?;
    let share = |from: &Kraddie| KraddieCommand::ShareMemory {
        reader: reader.id(),
        from: from.id(),
        switch: Switch::On,
    };
    assert_eq!(send(&runtime, share(&from))?, Reply::Accepted);
    let kraddies = kraddies_of(&runtime)?;
    let shared = kraddies
        .kraddies
        .iter()
        .find(|state| state.id() == reader.id())
        .ok_or("the reader is gone")?;
    assert!(shared.kraddie.settings.reads.contains(&from.id()));
    assert!(matches!(
        send(&runtime, share(&reader))?,
        Reply::Refused {
            code: ErrorCode::Invalid,
            ..
        }
    ));
    Ok(())
}
