use super::{
    claude_at, environment_at, harness, kraddies_of, start, wait_for, Folder, TestClock,
    TestResult, BULLETS_HARNESS, ENVIRONMENT,
};
use futures::executor::block_on;
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{AgentPreset, EnvironmentName, MemoryOwner, Prompt, Speaker, TerminalId};
use krad_protocol::{AgentRef, Scope};
use krad_runtime::{ChatBrief, ChatStart, KraddiesSetup};
use krad_store::OrdinaryDigests;

const CHAT: TerminalId = TerminalId(1);
const QUESTION: &str = "Why does it race?";
const KEPT_FACT: &str = "Two tabs rotate at once.";

#[test]
fn a_chat_the_runtime_closes_leaves_a_digest_of_what_was_said() -> TestResult {
    let folder = Folder::new("kraddie-digest")?;
    let memory = Folder::new("kraddie-digest-memory")?;
    let ordinary = OrdinaryDigests::in_folder(&memory.0);
    let setup = KraddiesSetup {
        shelf: None,
        ordinary: Some(ordinary.clone()),
        clock: TestClock::new().clock(),
    };
    let data = environment_at(&folder.0, &[CHAT.0])?;
    let runtime = start(data, claude_at(&harness(BULLETS_HARNESS)?), (setup, None))?;
    let chat = AgentRef {
        environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
        terminal: CHAT,
    };
    runtime.open_chat(
        chat.clone(),
        ChatStart {
            harness: AgentPreset::ClaudeCode,
            brief: ChatBrief::default(),
            prepared: None,
            session: LiveSession::Start(new_session_id()),
            messages: Vec::new(),
            queued: vec![Prompt::from(QUESTION)],
            notices: Vec::new(),
        },
    );
    wait_for(|| {
        let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
        Ok(snapshot
            .chats
            .iter()
            .find(|summary| summary.tail.chat == chat)
            .filter(|summary| !summary.state.working)
            .filter(|summary| {
                summary
                    .tail
                    .messages
                    .iter()
                    .any(|message| message.speaker == Speaker::Assistant)
            })
            .map(drop))
    })?;
    assert!(kraddies_of(&runtime)?
        .digests
        .iter()
        .all(|owned| owned.book.is_empty()));
    runtime.close_chat(chat);
    let book = wait_for(|| {
        Ok(kraddies_of(&runtime)?
            .digests
            .into_iter()
            .find(|owned| owned.owner == MemoryOwner::Ordinary && !owned.book.is_empty())
            .map(|owned| owned.book))
    })?;
    let [digest] = book.digests() else {
        return Err("one chat left more than one digest".into());
    };
    assert_eq!(digest.facts, [KEPT_FACT]);
    assert_eq!(digest.chat.terminal, CHAT);
    block_on(runtime.settle())?;
    assert_eq!(ordinary.read()?, book);
    Ok(())
}
