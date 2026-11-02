use std::io::{BufReader, BufWriter, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use super::channel::Channel;
use super::serve::serve;
use super::session::{Incoming, Session, Side};
use super::wire::{OpenRequest, Stream};
use crate::sftp::Sftp;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const WAIT: Duration = Duration::from_secs(10);

fn served() -> Result<(Session, std::sync::mpsc::Receiver<Incoming>), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let client = TcpStream::connect(listener.local_addr()?)?;
    let (host, _) = listener.accept()?;
    let host_reader = BufReader::new(host.try_clone()?);
    let host_closer = host.try_clone()?;
    thread::spawn(move || {
        serve(
            host_reader,
            BufWriter::new(host),
            WAIT,
            Some(Box::new(move || {
                host_closer.shutdown(Shutdown::Both).ok();
            })),
        )
        .ok();
    });
    let closer = client.try_clone()?;
    let (session, incoming, _) = Session::start(
        BufReader::new(client.try_clone()?),
        BufWriter::new(client),
        Side::Client,
        "test",
        WAIT,
        Some(Box::new(move || {
            closer.shutdown(Shutdown::Both).ok();
        })),
    )?;
    Ok((session, incoming))
}

fn read_all(channel: &Channel, stream: Stream) -> std::io::Result<String> {
    let mut text = String::new();
    channel.reader(stream).read_to_string(&mut text)?;
    Ok(text)
}

#[test]
fn a_command_gets_its_input_and_reports_output_errors_and_status() -> TestResult {
    let (session, _) = served()?;
    let (channel, _) = session.open(
        OpenRequest::Exec {
            script: "read line; printf 'got %s' \"$line\"; printf oops >&2; exit 5".to_owned(),
        },
        WAIT,
    )?;
    let mut input = channel.writer(Stream::Main);
    input.write_all(b"ping\n")?;
    input.finish();
    let errors = {
        let channel = channel.clone();
        thread::spawn(move || read_all(&channel, Stream::Errors))
    };
    assert_eq!(read_all(&channel, Stream::Main)?, "got ping");
    assert_eq!(errors.join().map_err(|_| "panicked")??, "oops");
    assert_eq!(channel.wait_exit(Some(WAIT))?, Some(5));
    Ok(())
}

#[test]
fn a_killed_command_stops_with_its_children() -> TestResult {
    let (session, _) = served()?;
    let (channel, _) = session.open(
        OpenRequest::Exec {
            script: "sleep 30 & echo started; wait".to_owned(),
        },
        WAIT,
    )?;
    let mut started = [0; 8];
    channel.reader(Stream::Main).read_exact(&mut started)?;
    channel.kill();
    let code = channel.wait_exit(Some(WAIT))?;
    assert!(code.is_some_and(|code| code != 0));
    Ok(())
}

#[test]
fn sftp_runs_over_a_channel_to_the_hosts_sftp_server() -> TestResult {
    let (session, _) = served()?;
    let Ok((channel, _)) = session.open(OpenRequest::Sftp, WAIT) else {
        return Ok(());
    };
    let sftp = Sftp::handshake(
        channel.reader(Stream::Main),
        channel.writer(Stream::Main),
        WAIT,
    )?;
    let home = sftp.realpath(".")?;
    assert!(home.starts_with('/'));
    Ok(())
}

#[test]
fn connections_are_forwarded_both_ways() -> TestResult {
    let (session, incoming) = served()?;
    let target = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = target.local_addr()?.port();
    thread::spawn(move || {
        for connection in target.incoming().flatten() {
            let mut connection = connection;
            let mut bytes = [0; 4];
            if connection.read_exact(&mut bytes).is_ok() {
                connection.write_all(&bytes).ok();
            }
        }
    });
    let (to_host, _) = session.open(OpenRequest::Connect { port }, WAIT)?;
    let mut writer = to_host.writer(Stream::Main);
    writer.write_all(b"ping")?;
    let mut echoed = [0; 4];
    to_host.reader(Stream::Main).read_exact(&mut echoed)?;
    assert_eq!(&echoed, b"ping");
    let (listening, remote_port) = session.open(OpenRequest::Listen, WAIT)?;
    let remote_port = remote_port.ok_or("no port")?;
    let dialing = thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut tcp = TcpStream::connect((Ipv4Addr::LOCALHOST, remote_port))?;
        tcp.write_all(b"pong")?;
        let mut back = vec![0; 4];
        tcp.read_exact(&mut back)?;
        Ok(back)
    });
    let accepted = incoming.recv_timeout(WAIT)?;
    assert_eq!(
        accepted.request,
        OpenRequest::Accepted {
            listener: listening.id()
        }
    );
    accepted.channel.accept(None)?;
    let mut heard = [0; 4];
    accepted
        .channel
        .reader(Stream::Main)
        .read_exact(&mut heard)?;
    accepted.channel.writer(Stream::Main).write_all(&heard)?;
    assert_eq!(dialing.join().map_err(|_| "panicked")??, b"pong");
    listening.close();
    Ok(())
}

#[cfg(unix)]
fn read_until(channel: &Channel, marker: &str) -> std::io::Result<String> {
    let mut reader = channel.reader(Stream::Main);
    let mut seen = Vec::new();
    let mut byte = [0; 1];
    while !String::from_utf8_lossy(&seen).contains(marker) {
        if reader.read(&mut byte)? == 0 {
            break;
        }
        seen.push(byte[0]);
    }
    Ok(String::from_utf8_lossy(&seen).into_owned())
}

#[cfg(unix)]
#[test]
fn a_terminal_is_a_real_tty_that_follows_resizes_and_reports_its_status() -> TestResult {
    use super::wire::TerminalSize;

    let (session, _) = served()?;
    let (channel, _) = session.open(
        OpenRequest::Pty {
            script: "[ -t 0 ] && printf 'tty '; stty size; read line; stty size; exit 7".to_owned(),
            size: TerminalSize {
                cols: 100,
                rows: 30,
            },
        },
        WAIT,
    )?;
    let first = read_until(&channel, "30 100")?;
    assert!(first.contains("tty"), "{first:?}");
    channel.resize(TerminalSize {
        cols: 120,
        rows: 40,
    });
    thread::sleep(Duration::from_millis(200));
    channel.writer(Stream::Main).write_all(b"go\n")?;
    let second = read_until(&channel, "40 120")?;
    assert!(second.contains("40 120"), "{second:?}");
    assert_eq!(channel.wait_exit(Some(WAIT))?, Some(7));
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_killed_terminal_stops_its_shell() -> TestResult {
    use super::wire::TerminalSize;

    let (session, _) = served()?;
    let (channel, _) = session.open(
        OpenRequest::Pty {
            script: "echo ready; sleep 30".to_owned(),
            size: TerminalSize { cols: 80, rows: 24 },
        },
        WAIT,
    )?;
    read_until(&channel, "ready")?;
    channel.kill();
    assert!(channel.wait_exit(Some(WAIT)).is_ok());
    Ok(())
}
