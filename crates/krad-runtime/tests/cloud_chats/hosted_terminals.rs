use futures::executor::block_on;
use futures::StreamExt as _;
use krad_client::{Runner, RuntimeClient};
use krad_domain::{AgentPreset, EnvironmentData, LaunchIntent, NodeEdit, NodeKind, SessionId};
use krad_protocol::{
    AgentRef, Answer, Command, EventKind, Feed, NodeCommand, Reply, Scope, TerminalCommand, WriteBy,
};
use std::error::Error;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use super::cloud::TestResult;

const WAIT: Duration = Duration::from_secs(30);
const ECHOED: &str = "krad-beside";
const NO_ANSWER: &str = "the runner never answered which terminal it opened";
const NO_SESSION: &str = "the hosted chat is in no session on the runner";
const NO_BYTES: &str = "the terminal opened beside the chat never showed the echo";
const NOT_PLACED: &str = "the new terminal is not in the chat's session";
const STILL_KEPT: &str = "the closed terminal is still kept on the runner";

fn runner_environment(client: &Runner, id: &AgentRef) -> Result<EnvironmentData, Box<dyn Error>> {
    let snapshot = block_on(client.snapshot(Scope::Runtime))?;
    Ok(snapshot
        .environments
        .into_iter()
        .find(|data| data.name == id.environment)
        .ok_or(NO_SESSION)?)
}

fn session_of(client: &Runner, id: &AgentRef) -> Result<Option<SessionId>, Box<dyn Error>> {
    Ok(runner_environment(client, id)?.session_of(id.terminal))
}

fn answers(client: &Runner) -> Receiver<Feed> {
    let mut stream = client.subscribe(None);
    let (sender, received) = channel();
    std::thread::spawn(move || {
        while let Some(feed) = block_on(stream.next()) {
            if sender.send(feed).is_err() {
                break;
            }
        }
    });
    received
}

fn opened_beside(client: &Runner, chat: &AgentRef) -> Result<AgentRef, Box<dyn Error>> {
    let heard = answers(client);
    let request = client.next_request();
    let open = Command::Terminal(TerminalCommand::OpenBeside {
        beside: chat.clone(),
        intent: LaunchIntent::Preset {
            preset: AgentPreset::Shell,
        },
    });
    assert_eq!(block_on(client.send_as(request, open))?, Reply::Accepted);
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let Feed::Event(event) = heard.recv_timeout(left).map_err(|_| NO_ANSWER)? else {
            continue;
        };
        if let EventKind::Answered {
            request: answered,
            answer,
        } = event.kind
        {
            if answered != request {
                continue;
            }
            let Answer::TerminalOpened { terminal } = *answer else {
                return Err(NO_ANSWER.into());
            };
            return Ok(terminal);
        }
    }
}

fn echoes(client: &Runner, terminal: &AgentRef) -> TestResult {
    let (reply, mut bytes) = client.watch_terminal(terminal.clone());
    assert_eq!(block_on(reply)?, Reply::Accepted);
    let write = TerminalCommand::Write {
        terminal: terminal.clone(),
        bytes: format!("echo {ECHOED}\n").into_bytes(),
        by: WriteBy::User,
    };
    assert_eq!(
        block_on(client.send(Command::Terminal(write)))?,
        Reply::Accepted
    );
    let (seen, reading) = channel();
    std::thread::spawn(move || {
        let mut shown = Vec::new();
        while let Some(chunk) = block_on(bytes.next()) {
            shown.extend(chunk);
            if String::from_utf8_lossy(&shown).matches(ECHOED).count() >= 2 {
                seen.send(()).unwrap_or_default();
                return;
            }
        }
    });
    reading.recv_timeout(WAIT).map_err(|_| NO_BYTES)?;
    Ok(())
}

fn closes_on_the_runner(client: &Runner, terminal: &AgentRef) -> TestResult {
    let close = Command::Terminal(TerminalCommand::Close {
        terminal: terminal.clone(),
    });
    let forget = Command::Node(NodeCommand {
        environment: terminal.environment.clone(),
        edit: NodeEdit::Close {
            terminal: terminal.terminal,
        },
    });
    for command in [close, forget] {
        assert_eq!(block_on(client.send(command))?, Reply::Accepted);
    }
    let data = runner_environment(client, terminal)?;
    if data.nodes.iter().any(|node| node.id == terminal.terminal) {
        return Err(STILL_KEPT.into());
    }
    Ok(())
}

pub fn a_terminal_opened_beside_a_hosted_chat_runs_in_its_session_until_closed(
    client: &Runner,
    chat: &AgentRef,
) -> TestResult {
    let session = session_of(client, chat)?.ok_or(NO_SESSION)?;
    let terminal = opened_beside(client, chat)?;
    let data = runner_environment(client, &terminal)?;
    let node = data
        .nodes
        .iter()
        .find(|node| node.id == terminal.terminal)
        .ok_or(NOT_PLACED)?;
    assert_eq!(node.kind, NodeKind::Terminal);
    assert_eq!(
        data.session_of(terminal.terminal),
        Some(session),
        "{NOT_PLACED}"
    );
    echoes(client, &terminal)?;
    closes_on_the_runner(client, &terminal)
}
