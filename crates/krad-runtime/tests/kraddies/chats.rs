use super::{
    claude_at, environment_at, environment_of, harness, start, wait_for, Folder, TestClock,
    TestResult, BULLETS_HARNESS, ENVIRONMENT,
};
use futures::executor::block_on;
use krad_agents::{call, Caller, Request, Response, TerminalHandle};
use krad_domain::{AgentPreset, EnvironmentName, Kraddie, NodeKind, SessionId, TerminalId};
use krad_protocol::{Command, KraddieCommand, Reply, Scope};
use krad_runtime::{BusSetup, KraddiesSetup, RuntimeHandle};
use krad_store::KraddieShelf;
use std::error::Error;

const SOME_TERMINAL: u64 = 99;

fn chat_is_open(runtime: &RuntimeHandle, terminal: TerminalId) -> Result<bool, Box<dyn Error>> {
    Ok(block_on(runtime.snapshot(Scope::Runtime))?
        .chats
        .iter()
        .any(|summary| summary.tail.chat.terminal == terminal))
}

fn kraddie_chat(
    runtime: &RuntimeHandle,
    kraddie: &Kraddie,
) -> Result<Option<(SessionId, TerminalId)>, Box<dyn Error>> {
    let data = environment_of(runtime)?;
    let id = kraddie.id();
    let Some(session) = data.kraddie_sessions(&id).next() else {
        return Ok(None);
    };
    let chat = data
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Chat && data.session_of(node.id) == Some(session.id))
        .map(|node| (session.id, node.id));
    Ok(chat)
}

#[test]
fn a_kraddie_chat_opens_in_the_runtime_from_a_client_and_from_the_bus() -> TestResult {
    let folder = Folder::new("kraddie-chat")?;
    let shelf = KraddieShelf::at(folder.0.join("shelf"));
    let kraddie = Kraddie::new("Night Owl".parse()?, AgentPreset::ClaudeCode);
    shelf.write(&kraddie)?;
    let setup = KraddiesSetup {
        shelf: Some(shelf),
        ordinary: None,
        clock: TestClock::new().clock(),
    };
    let runtime = start(
        environment_at(&folder.0, &[])?,
        claude_at(&harness(BULLETS_HARNESS)?),
        (setup, Some(BusSetup::default())),
    )?;
    let open = Command::Kraddie(KraddieCommand::OpenChat {
        kraddie: kraddie.id(),
        environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
    });
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), open))?,
        Reply::Accepted
    );
    let (session, terminal) = wait_for(|| kraddie_chat(&runtime, &kraddie))?;
    assert!(chat_is_open(&runtime, terminal)?);
    assert_eq!(environment_of(&runtime)?.active_session, Some(session));
    let endpoint = &runtime.bus().ok_or("no bus")?.endpoint;
    let (address, token) = (endpoint.address.to_string(), endpoint.token.clone());
    let caller = Caller::Terminal(TerminalHandle {
        environment: ENVIRONMENT.to_owned(),
        terminal: SOME_TERMINAL,
    });
    let request = Request::NewKraddieChat {
        name: "Night Owl".to_owned(),
    };
    let answer = std::thread::spawn(move || call(&address, &token, &caller, request))
        .join()
        .map_err(|_| "the caller panicked")??;
    let Response::KraddieChatOpened(opened) = answer else {
        return Err(format!("the bus did not open a chat: {answer:?}").into());
    };
    let second = TerminalId(opened.terminal);
    assert_ne!(second, terminal);
    assert_eq!(
        environment_of(&runtime)?.session_of(second),
        Some(SessionId(opened.session))
    );
    assert!(chat_is_open(&runtime, second)?);
    Ok(())
}
