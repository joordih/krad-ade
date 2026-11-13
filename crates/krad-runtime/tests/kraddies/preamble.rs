use super::{
    claude_at, environment_at, environment_of, harness, start, wait_for, Folder, TestClock,
    TestResult, ENVIRONMENT,
};
use futures::executor::block_on;
use krad_agents::{call, Caller, Request, Response, TerminalHandle};
use krad_domain::{
    AgentPreset, DigestBook, DigestChat, DigestDraft, EnvironmentEdit, EnvironmentName,
    SessionEdit, SessionId, SessionLead, TerminalId,
};
use krad_protocol::Command;
use krad_runtime::{BusSetup, KraddiesSetup};
use krad_store::OrdinaryDigests;
use std::time::SystemTime;

const ARGV_HARNESS: &str = "tests/fixtures/fake-harness/claude-argv";
const ARGV_FILE_VAR: &str = "KRAD_TEST_ARGV_FILE";
const LEAD: TerminalId = TerminalId(1);
const REMEMBERED: &str = "Staging deploys from the release branch";
const TASK: &str = "Check the staging deploy";
const SYSTEM_PROMPT_FLAG: &str = "--append-system-prompt";

fn remembered(memory: &OrdinaryDigests) -> Result<(), Box<dyn std::error::Error>> {
    let mut book = DigestBook::default();
    book.record(DigestDraft {
        chat: DigestChat {
            environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
            session: SessionId(40),
            terminal: TerminalId(40),
            title: "Staging".to_owned(),
            folder: None,
            resume: None,
        },
        facts: vec![REMEMBERED.to_owned()],
        covered: 2,
        written: SystemTime::now(),
    })
    .ok_or("the digest was not kept")?;
    memory.write(&book)?;
    Ok(())
}

#[test]
fn a_worker_a_lead_starts_in_the_runtime_launches_with_the_memory_preamble() -> TestResult {
    let folder = Folder::new("kraddie-preamble")?;
    let argv = folder.0.join("argv");
    std::env::set_var(ARGV_FILE_VAR, &argv);
    let memory = OrdinaryDigests::in_folder(&folder.0);
    remembered(&memory)?;
    let setup = KraddiesSetup {
        shelf: None,
        ordinary: Some(memory),
        clock: TestClock::new().clock(),
    };
    let data = environment_at(&folder.0, &[LEAD.0])?;
    let session = data.active_session.ok_or("no session to lead")?;
    let runtime = start(
        data,
        claude_at(&harness(ARGV_HARNESS)?),
        (setup, Some(BusSetup::default())),
    )?;
    let lead: Command = (
        EnvironmentName::new(ENVIRONMENT.to_owned()),
        EnvironmentEdit::from(SessionEdit::MakeLead {
            session,
            lead: SessionLead {
                terminal: Some(LEAD),
                ..SessionLead::new(AgentPreset::ClaudeCode, None)
            },
        }),
    )
        .into();
    block_on(runtime.send(runtime.next_request(), lead))?;
    let endpoint = &runtime.bus().ok_or("no bus")?.endpoint;
    let caller = Caller::Terminal(TerminalHandle {
        environment: ENVIRONMENT.to_owned(),
        terminal: LEAD.0,
    });
    let spawn = Request::Spawn {
        agent: AgentPreset::ClaudeCode,
        worktree: None,
        task: TASK.to_owned(),
    };
    let (address, token) = (endpoint.address.to_string(), endpoint.token.clone());
    let spawned = std::thread::spawn(move || call(&address, &token, &caller, spawn))
        .join()
        .map_err(|_| "the caller panicked")??;
    let Response::Spawned { terminal } = spawned else {
        return Err("the lead's worker did not start".into());
    };
    assert!(environment_of(&runtime)?
        .nodes
        .iter()
        .any(|node| node.id == TerminalId(terminal)));
    let launched = wait_for(|| {
        let recorded = std::fs::read_to_string(&argv).unwrap_or_default();
        Ok(recorded.contains(SYSTEM_PROMPT_FLAG).then_some(recorded))
    })?;
    assert!(launched.contains(REMEMBERED), "{launched}");
    Ok(())
}
