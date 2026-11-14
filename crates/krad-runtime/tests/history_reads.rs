#![cfg(unix)]

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    AgentPreset, ChatEntry, EnvironmentData, EnvironmentName, ResumeId, RuntimeState, TerminalId,
};
use krad_protocol::{
    Answer, ChatCommand, Command, ErrorCode, EventKind, Feed, HistoryAsk, HistoryCursor,
    HistoryPart, Reply,
};
use krad_runtime::{EnvironmentsSetup, EventReceiver, Runtime, RuntimeHandle, RuntimeSetup};
use std::error::Error;
use std::path::PathBuf;

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const HOME_VAR: &str = "HOME";
const PROJECTS: &str = ".claude/projects/history-reads";
const RESUME: &str = "history-reads-chat";
const QUESTION: &str = "What reads it?";
const LATER: &str = "And then?";
const LINE_END: char = '\n';

struct Home(PathBuf);

impl Drop for Home {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn main_name() -> EnvironmentName {
    EnvironmentName::new(ENVIRONMENT.to_owned())
}

fn user_line(text: &str) -> String {
    format!(
        r#"{{"type":"user","message":{{"role":"user","content":[{{"type":"text","text":"{text}"}}]}}}}"#
    )
}

fn start() -> Result<(RuntimeHandle, EventReceiver), Box<dyn Error>> {
    let runtime = Runtime::start(RuntimeSetup {
        environments: EnvironmentsSetup {
            state: RuntimeState {
                environments: vec![EnvironmentData::empty(main_name())],
                ..RuntimeState::default()
            },
            file: None,
        },
        ..RuntimeSetup::default()
    })?;
    let mut feed = runtime.subscribe();
    block_on(feed.next()).ok_or("no snapshot")?;
    Ok((runtime, feed))
}

fn history(harness: AgentPreset, cursor: HistoryCursor) -> Result<Command, Box<dyn Error>> {
    Ok(Command::Chat(ChatCommand::History {
        environment: main_name(),
        ask: HistoryAsk {
            chat: TerminalId(1),
            harness,
            resume: RESUME.parse::<ResumeId>().map_err(|_| "resume id")?,
            root: None,
            cursor,
        },
    }))
}

fn read(
    runtime: &RuntimeHandle,
    feed: &mut EventReceiver,
    cursor: HistoryCursor,
) -> Result<(Vec<ChatEntry>, HistoryCursor), Box<dyn Error>> {
    let request = runtime.next_request();
    let reply = block_on(runtime.send(request, history(AgentPreset::ClaudeCode, cursor)?))?;
    if reply != Reply::Accepted {
        return Err(format!("{reply:?}").into());
    }
    while let Some(Feed::Event(event)) = block_on(feed.next()) {
        let EventKind::Answered {
            request: heard,
            answer,
        } = event.kind
        else {
            continue;
        };
        if heard != request {
            continue;
        }
        return match *answer {
            Answer::History(part) => match *part {
                HistoryPart::Appended { entries, cursor } => Ok((entries, cursor)),
                HistoryPart::Whole { .. } => Err("a whole read for Claude Code".into()),
            },
            other => Err(format!("{other:?}").into()),
        };
    }
    Err("the runtime stopped".into())
}

fn user_texts(entries: &[ChatEntry]) -> Vec<&str> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            ChatEntry::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_claude_history_is_read_on_from_where_the_last_read_stopped() -> TestResult {
    let home =
        Home(std::env::temp_dir().join(format!("krad-history-reads-{}", std::process::id())));
    std::fs::create_dir_all(home.0.join(PROJECTS))?;
    std::env::set_var(HOME_VAR, &home.0);
    let conversation = home.0.join(PROJECTS).join(format!("{RESUME}.jsonl"));
    std::fs::write(&conversation, format!("{}\n", user_line(QUESTION)))?;
    let (runtime, mut feed) = start()?;

    let (entries, cursor) = read(&runtime, &mut feed, HistoryCursor::default())?;
    assert_eq!(user_texts(&entries), [QUESTION]);
    assert_eq!(cursor.file.as_deref(), Some(conversation.as_path()));
    assert!(cursor.next > 0);

    let mut grown = std::fs::read_to_string(&conversation)?;
    grown.push_str(&user_line(LATER));
    grown.push(LINE_END);
    std::fs::write(&conversation, grown)?;
    let (entries, _) = read(&runtime, &mut feed, cursor)?;
    assert_eq!(user_texts(&entries), [LATER]);

    let shell = block_on(runtime.send(
        runtime.next_request(),
        history(AgentPreset::Shell, HistoryCursor::default())?,
    ))?;
    assert!(matches!(
        shell,
        Reply::Refused {
            code: ErrorCode::Unsupported,
            ..
        }
    ));
    Ok(())
}
