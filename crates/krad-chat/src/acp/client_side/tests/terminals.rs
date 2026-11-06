use super::*;
use crate::acp::connection::{AcpClient, AcpConnection};
use crate::acp::ids::TerminalId;
use crate::acp::terminals::TerminalExitStatus;
use crate::acp::update::SessionNotification;
use serde_json::{json, Value};
use std::io::{pipe, BufRead, BufReader, Write};
use std::time::Instant;

#[test]
fn a_terminal_runs_in_its_folder_with_its_env_and_reports_its_exit() -> TestResult {
    let scratch = Scratch::new();
    scratch.file("marker.txt", "");
    let terminals = HostTerminals::new(HostLink::Local, AgentPaths::on_host(scratch.0.clone()));
    let id = terminals
        .create(&create("ls; echo $KRAD_ACP_VALUE; exit 3", None, None))?
        .terminal_id;
    let exit = terminals.wait_for_exit(&id)?;
    assert_eq!(
        exit,
        TerminalExitStatus {
            exit_code: Some(3),
            signal: None
        }
    );
    let output = terminals.output(&id)?;
    assert_eq!(output.output, "marker.txt\nfrom-env\n");
    assert!(!output.truncated);
    assert_eq!(output.exit_status, Some(exit));
    assert_eq!(terminals.snapshot(&id), Some(output));
    Ok(())
}

#[test]
fn output_over_the_byte_limit_keeps_its_end() -> TestResult {
    let scratch = Scratch::new();
    let terminals = HostTerminals::new(HostLink::Local, AgentPaths::on_host(scratch.0.clone()));
    let id = terminals
        .create(&create(
            "printf 'start-aéb'",
            Some(scratch.0.clone()),
            Some(2),
        ))?
        .terminal_id;
    terminals.wait_for_exit(&id)?;
    let output = terminals.output(&id)?;
    assert!(output.truncated);
    assert_eq!(output.output, "b");
    Ok(())
}

#[test]
fn a_killed_terminal_exits_by_signal_and_stays_readable() -> TestResult {
    let scratch = Scratch::new();
    let terminals = HostTerminals::new(HostLink::Local, AgentPaths::on_host(scratch.0.clone()));
    let id = terminals
        .create(&create("echo begun; sleep 30", None, None))?
        .terminal_id;
    let started = Instant::now();
    terminals.kill(&id)?;
    let exit = terminals.wait_for_exit(&id)?;
    assert_eq!(exit.signal.as_deref(), Some("SIGKILL"));
    assert!(started.elapsed() < WAIT);
    assert!(terminals.output(&id).is_ok());
    Ok(())
}

#[test]
fn a_released_terminal_is_killed_and_forgotten_by_the_agent() -> TestResult {
    let scratch = Scratch::new();
    let terminals = HostTerminals::new(HostLink::Local, AgentPaths::on_host(scratch.0.clone()));
    let id = terminals
        .create(&create("sleep 30", None, None))?
        .terminal_id;
    terminals.release(&id)?;
    let refused = terminals.output(&id).err().map(|error| error.code);
    assert_eq!(refused, Some(ErrorCode::INVALID_PARAMS));
    let deadline = Instant::now() + WAIT;
    while terminals
        .snapshot(&id)
        .is_some_and(|snapshot| snapshot.exit_status.is_none())
    {
        assert!(
            Instant::now() < deadline,
            "the released terminal kept running"
        );
        thread::sleep(Duration::from_millis(20));
    }
    let unknown = TerminalId("terminal-unknown".to_owned());
    assert!(terminals.kill(&unknown).is_err());
    Ok(())
}

struct Served(ClientSide);

impl AcpClient for Served {
    fn session_update(&mut self, _notification: SessionNotification) {}

    fn request(&mut self, request: AgentRequest) {
        drop(self.0.serve(request));
    }

    fn closed(&mut self) {}
}

#[test]
fn the_agent_gets_its_answers_over_json_rpc() -> TestResult {
    let scratch = Scratch::new();
    let path = scratch.file("a.txt", "hello\n");
    let (sender, _written) = channel();
    let client = ClientSide::new(
        HostLink::Local,
        AgentPaths::on_host(scratch.0.clone()),
        Arc::new(FakeBuffers::default()),
        sender,
    );
    let (from_agent, mut to_client) = pipe()?;
    let (from_client, to_agent) = pipe()?;
    let _connection = AcpConnection::start(from_agent, to_agent, Served(client));
    let mut answers = BufReader::new(from_client);
    let mut ask = |id: u64, method: &str, params: Value| -> Result<Value, Box<dyn Error>> {
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(to_client, "{line}")?;
        let mut answer = String::new();
        answers.read_line(&mut answer)?;
        Ok(serde_json::from_str(&answer)?)
    };
    let read = ask(
        1,
        "fs/read_text_file",
        json!({"sessionId": SESSION, "path": path}),
    )?;
    assert_eq!(read["result"]["content"], "hello\n");
    let relative = ask(
        2,
        "fs/read_text_file",
        json!({"sessionId": SESSION, "path": "a.txt"}),
    )?;
    assert_eq!(relative["error"]["code"], ErrorCode::INVALID_PARAMS.0);
    let created = ask(
        3,
        "terminal/create",
        json!({"sessionId": SESSION, "command": "sh", "args": ["-c", "echo hi"]}),
    )?;
    let terminal_id = created["result"]["terminalId"].clone();
    let exit = ask(
        4,
        "terminal/wait_for_exit",
        json!({"sessionId": SESSION, "terminalId": terminal_id}),
    )?;
    assert_eq!(exit["result"]["exitCode"], 0);
    let output = ask(
        5,
        "terminal/output",
        json!({"sessionId": SESSION, "terminalId": terminal_id}),
    )?;
    assert_eq!(output["result"]["output"], "hi\n");
    let released = ask(
        6,
        "terminal/release",
        json!({"sessionId": SESSION, "terminalId": terminal_id}),
    )?;
    assert_eq!(released["result"], json!({}));
    Ok(())
}
