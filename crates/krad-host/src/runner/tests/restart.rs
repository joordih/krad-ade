use krad_domain::{CloudProcessId, CloudProcessRun, JournalOffset};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use super::super::process::group_alive;
use super::{attach, list, open, running_daemon, script, serve, Frame, Runner, TestResult};
use crate::mux::TerminalSize;

const PATIENCE: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(20);

fn read_until(stream: &mut UnixStream, wanted: &[u8], seen: &mut Vec<u8>) -> TestResult {
    while !seen.windows(wanted.len()).any(|window| window == wanted) {
        match Frame::read(stream)? {
            Frame::Data(bytes) => seen.extend_from_slice(&bytes),
            Frame::Exit(code) => return Err(format!("ended early with {code:?}").into()),
        }
    }
    Ok(())
}

fn run_of(
    runner: &Runner,
    id: &CloudProcessId,
) -> Result<(CloudProcessRun, u64), Box<dyn std::error::Error>> {
    list(&runner.socket, &runner.pairing)?
        .into_iter()
        .find(|status| &status.id == id)
        .map(|status| (status.run, status.end.0))
        .ok_or_else(|| "not listed".into())
}

#[test]
fn a_piped_process_survives_a_daemon_restart_and_takes_input_again() -> TestResult {
    let runner = running_daemon("restart")?;
    let id: CloudProcessId = "chat-restart".parse()?;
    let mut stream = open(
        &runner,
        &id,
        script(
            &runner.root,
            "echo start; read line; echo \"got $line\"; read second; echo \"then $second\"; exit 7",
        ),
    )?;
    let mut seen = Vec::new();
    read_until(&mut stream, b"start\n", &mut seen)?;
    stream.write_all(b"one\n")?;
    read_until(&mut stream, b"got one\n", &mut seen)?;
    drop(stream);
    serve(&runner.root)?;
    assert_eq!(run_of(&runner, &id)?, (CloudProcessRun::Running, 14));
    let mut output = Vec::new();
    let code = attach(
        &runner.socket,
        &runner.pairing,
        id.clone(),
        JournalOffset(6),
        None,
        &b"two\n"[..],
        &mut output,
    )?;
    assert_eq!(code, Some(7));
    assert_eq!(output, b"krad-journal 6 14\ngot one\nthen two\n");
    assert_eq!(run_of(&runner, &id)?, (CloudProcessRun::Ended(Some(7)), 23));
    Ok(())
}

#[test]
fn a_terminal_left_by_an_earlier_daemon_is_stopped_and_listed_ended() -> TestResult {
    let runner = running_daemon("orphan")?;
    let id: CloudProcessId = "shell-orphan".parse()?;
    let mut launch = script(&runner.root, "read go");
    launch.terminal = Some(TerminalSize { cols: 80, rows: 24 });
    let stream = open(&runner, &id, launch)?;
    let group: u32 = std::fs::read_to_string(runner.root.join("processes/shell-orphan/pid"))?
        .trim()
        .parse()?;
    assert!(group_alive(group));
    serve(&runner.root)?;
    assert!(matches!(run_of(&runner, &id)?.0, CloudProcessRun::Ended(_)));
    let started = Instant::now();
    while group_alive(group) {
        assert!(started.elapsed() < PATIENCE, "the terminal was not stopped");
        std::thread::sleep(POLL);
    }
    drop(stream);
    Ok(())
}
