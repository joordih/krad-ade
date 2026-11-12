use krad_domain::{CloudProcessId, CloudProcessRun, InstallId, JournalOffset, Pairing};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use super::clients::pair;
use super::wire::{Envelope, Frame, Reply, Request, PROTOCOL_VERSION};
use super::{attach, list, resize, runner_socket, stop, CloudLaunch, RunnerDaemon};
use crate::mux::TerminalSize;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Runner {
    root: PathBuf,
    socket: PathBuf,
    pairing: Pairing,
}

impl Drop for Runner {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn serve(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let daemon = RunnerDaemon::new(root);
    let listener = daemon.bind()?;
    let served = Arc::clone(&daemon);
    thread::spawn(move || served.serve(&listener));
    Ok(())
}

fn running_daemon(name: &str) -> Result<Runner, Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("krad-runner-{name}-{}", std::process::id()));
    serve(&root)?;
    let install: InstallId = "0123456789abcdef".parse()?;
    let token = pair(&root, &install)?;
    Ok(Runner {
        socket: runner_socket(&root),
        root,
        pairing: Pairing { install, token },
    })
}

fn script(root: &Path, text: &str) -> CloudLaunch {
    CloudLaunch {
        cwd: root.to_owned(),
        program: "/bin/sh".to_owned(),
        args: vec!["-c".to_owned(), text.to_owned()],
        env: vec![("GREETING".to_owned(), "hi".to_owned())],
        terminal: None,
    }
}

fn open(
    runner: &Runner,
    id: &CloudProcessId,
    launch: CloudLaunch,
) -> Result<UnixStream, Box<dyn std::error::Error>> {
    let mut stream = UnixStream::connect(&runner.socket)?;
    Envelope {
        pairing: runner.pairing.clone(),
        request: Request::Open {
            id: id.clone(),
            from: JournalOffset::default(),
            launch: Some(launch),
        },
    }
    .write(&mut stream)?;
    assert_eq!(
        Reply::read(&mut stream)?,
        Reply::Accepted {
            end: JournalOffset::default()
        }
    );
    Ok(stream)
}

#[test]
fn a_process_replays_its_journal_after_a_header_from_any_offset() -> TestResult {
    let runner = running_daemon("replay")?;
    let id: CloudProcessId = "chat-replay".parse()?;
    let mut first = Vec::new();
    let code = attach(
        &runner.socket,
        &runner.pairing,
        id.clone(),
        JournalOffset::default(),
        Some(script(
            &runner.root,
            "echo \"$GREETING\"; read line; echo \"got $line\"",
        )),
        &b"hello\n"[..],
        &mut first,
    )?;
    assert_eq!(code, Some(0));
    assert_eq!(first, b"krad-journal 0 0\nhi\ngot hello\n");
    let mut tail = Vec::new();
    let code = attach(
        &runner.socket,
        &runner.pairing,
        id,
        JournalOffset(3),
        None,
        &b""[..],
        &mut tail,
    )?;
    assert_eq!(code, Some(0));
    assert_eq!(tail, b"krad-journal 3 13\ngot hello\n");
    Ok(())
}

#[test]
fn a_process_keeps_running_after_its_client_detaches() -> TestResult {
    let runner = running_daemon("detach")?;
    let id: CloudProcessId = "chat-detach".parse()?;
    drop(open(
        &runner,
        &id,
        script(&runner.root, "read line; echo \"late $line\""),
    )?);
    let mut output = Vec::new();
    let code = attach(
        &runner.socket,
        &runner.pairing,
        id,
        JournalOffset::default(),
        None,
        &b"answer\n"[..],
        &mut output,
    )?;
    assert_eq!(code, Some(0));
    assert_eq!(output, b"krad-journal 0 0\nlate answer\n");
    Ok(())
}

#[test]
fn an_unknown_process_or_a_future_offset_is_refused() -> TestResult {
    let runner = running_daemon("refuse")?;
    let unknown = attach(
        &runner.socket,
        &runner.pairing,
        "nobody".parse()?,
        JournalOffset::default(),
        None,
        &b""[..],
        &mut Vec::new(),
    );
    assert!(unknown.is_err());
    let id: CloudProcessId = "chat-short".parse()?;
    attach(
        &runner.socket,
        &runner.pairing,
        id.clone(),
        JournalOffset::default(),
        Some(script(&runner.root, "echo ok")),
        &b""[..],
        &mut Vec::new(),
    )?;
    let mut told = Vec::new();
    let ahead = attach(
        &runner.socket,
        &runner.pairing,
        id,
        JournalOffset(99),
        None,
        &b""[..],
        &mut told,
    );
    assert!(ahead.is_err());
    assert_eq!(told, b"krad-journal-past-end 3\n");
    Ok(())
}

#[test]
fn only_a_paired_installation_with_its_token_is_served() -> TestResult {
    let runner = running_daemon("auth")?;
    assert!(list(&runner.socket, &runner.pairing).is_ok());
    let wrong_token = Pairing {
        install: runner.pairing.install.clone(),
        token: "ffffffffffffffffffffffffffffffff".parse()?,
    };
    let refused = list(&runner.socket, &wrong_token).err();
    assert!(refused.is_some_and(|error| error.to_string().contains("not paired")));
    let stranger = Pairing {
        install: "fedcba9876543210".parse()?,
        token: runner.pairing.token.clone(),
    };
    assert!(list(&runner.socket, &stranger).is_err());
    Ok(())
}

#[test]
fn a_client_speaking_another_protocol_is_told_why() -> TestResult {
    let runner = running_daemon("version")?;
    let mut bytes = Vec::new();
    Envelope {
        pairing: runner.pairing.clone(),
        request: Request::List,
    }
    .write(&mut bytes)?;
    bytes[0] = PROTOCOL_VERSION - 1;
    let mut stream = UnixStream::connect(&runner.socket)?;
    stream.write_all(&bytes)?;
    let Reply::Refused(reason) = Reply::read(&mut stream)? else {
        return Err("the daemon accepted another protocol".into());
    };
    assert!(reason.contains("protocol"));
    Ok(())
}

#[test]
fn the_list_shows_running_ended_and_orphaned_processes() -> TestResult {
    let runner = running_daemon("list")?;
    let done: CloudProcessId = "chat-done".parse()?;
    attach(
        &runner.socket,
        &runner.pairing,
        done.clone(),
        JournalOffset::default(),
        Some(script(&runner.root, "printf abc; exit 3")),
        &b""[..],
        &mut Vec::new(),
    )?;
    let waiting: CloudProcessId = "chat-waiting".parse()?;
    let stream = open(&runner, &waiting, script(&runner.root, "read line"))?;
    let pid = std::fs::read_to_string(runner.root.join("processes/chat-waiting/pid"))?;
    assert!(pid.parse::<u32>()? > 0);
    let statuses = list(&runner.socket, &runner.pairing)?;
    let runs: Vec<_> = statuses
        .iter()
        .map(|status| (status.id.as_str(), status.run, status.end.0))
        .collect();
    assert_eq!(
        runs,
        [
            ("chat-done", CloudProcessRun::Ended(Some(3)), 3),
            ("chat-waiting", CloudProcessRun::Running, 0),
        ]
    );
    serve(&runner.root)?;
    let restarted = list(&runner.socket, &runner.pairing)?;
    assert!(restarted
        .iter()
        .all(|status| status.run == CloudProcessRun::Ended(None)));
    assert_eq!(restarted.len(), 2);
    drop(stream);
    Ok(())
}

#[test]
fn a_terminal_process_takes_a_new_size() -> TestResult {
    let runner = running_daemon("resize")?;
    let id: CloudProcessId = "shell-resize".parse()?;
    let mut launch = script(&runner.root, "read go; stty size");
    launch.terminal = Some(TerminalSize { cols: 80, rows: 24 });
    let mut stream = open(&runner, &id, launch)?;
    resize(
        &runner.socket,
        &runner.pairing,
        id.clone(),
        TerminalSize {
            cols: 100,
            rows: 30,
        },
    )?;
    stream.write_all(b"go\n")?;
    let mut output = Vec::new();
    let code = loop {
        match Frame::read(&mut stream)? {
            Frame::Data(bytes) => output.extend_from_slice(&bytes),
            Frame::Exit(code) => break code,
        }
    };
    assert_eq!(code, Some(0));
    assert!(String::from_utf8_lossy(&output).contains("30 100"));
    let piped: CloudProcessId = "chat-piped".parse()?;
    drop(open(&runner, &piped, script(&runner.root, "read line"))?);
    let size = TerminalSize { cols: 1, rows: 1 };
    assert!(resize(&runner.socket, &runner.pairing, piped.clone(), size).is_err());
    stop(&runner.socket, &runner.pairing, piped)?;
    Ok(())
}

#[test]
fn every_process_knows_its_own_cloud_process_id() -> TestResult {
    let runner = running_daemon("marked")?;
    let id: CloudProcessId = "chat-marked".parse()?;
    let mut launch = script(&runner.root, "printf %s \"$KRAD_ADE_CLOUD_PROCESS\"");
    launch
        .env
        .push(("KRAD_ADE_CLOUD_PROCESS".to_owned(), "forged".to_owned()));
    let mut output = Vec::new();
    attach(
        &runner.socket,
        &runner.pairing,
        id,
        JournalOffset::default(),
        Some(launch),
        &b""[..],
        &mut output,
    )?;
    assert_eq!(output, b"krad-journal 0 0\nchat-marked");
    Ok(())
}
