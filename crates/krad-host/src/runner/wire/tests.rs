use super::*;
use krad_domain::CloudProcessRun;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn pairing() -> Result<Pairing, Box<dyn std::error::Error>> {
    Ok(Pairing {
        install: "0123456789abcdef".parse()?,
        token: "00112233445566778899aabbccddeeff".parse()?,
    })
}

fn round_trip_request(request: Request) -> TestResult {
    let envelope = Envelope {
        pairing: pairing()?,
        request,
    };
    let mut bytes = Vec::new();
    envelope.write(&mut bytes)?;
    assert_eq!(Envelope::read(&mut bytes.as_slice())?, envelope);
    Ok(())
}

#[test]
fn every_request_survives_the_wire_with_its_pairing() -> TestResult {
    let size = TerminalSize {
        cols: 120,
        rows: 40,
    };
    round_trip_request(Request::Open {
        id: "chat-1".parse()?,
        from: JournalOffset(42),
        launch: Some(CloudLaunch {
            cwd: PathBuf::from("/home/me/repo"),
            program: "/usr/bin/claude".to_owned(),
            args: vec!["--print".to_owned(), String::new()],
            env: vec![("KEY".to_owned(), "value".to_owned())],
            terminal: Some(size),
        }),
    })?;
    round_trip_request(Request::Open {
        id: "chat-1".parse()?,
        from: JournalOffset::default(),
        launch: None,
    })?;
    round_trip_request(Request::Stop {
        id: "chat-1".parse()?,
    })?;
    round_trip_request(Request::List)?;
    round_trip_request(Request::Resize {
        id: "shell-2".parse()?,
        size,
    })
}

#[test]
fn every_reply_and_frame_survives_the_wire() -> TestResult {
    for reply in [
        Reply::Accepted {
            end: JournalOffset(7),
        },
        Reply::Refused("no".to_owned()),
        Reply::PastEnd {
            end: JournalOffset(5),
        },
        Reply::Done,
        Reply::Listed(vec![
            CloudProcessStatus {
                id: "chat-1".parse()?,
                run: CloudProcessRun::Running,
                end: JournalOffset(3),
            },
            CloudProcessStatus {
                id: "chat-2".parse()?,
                run: CloudProcessRun::Ended(Some(-1)),
                end: JournalOffset(0),
            },
            CloudProcessStatus {
                id: "chat-3".parse()?,
                run: CloudProcessRun::Ended(None),
                end: JournalOffset(9),
            },
        ]),
    ] {
        let mut bytes = Vec::new();
        reply.write(&mut bytes)?;
        assert_eq!(Reply::read(&mut bytes.as_slice())?, reply);
    }
    for frame in [
        Frame::Data(b"out".to_vec()),
        Frame::Exit(None),
        Frame::Exit(Some(-3)),
    ] {
        let mut bytes = Vec::new();
        frame.write(&mut bytes)?;
        assert_eq!(Frame::read(&mut bytes.as_slice())?, frame);
    }
    Ok(())
}

#[test]
fn another_protocol_version_is_refused_with_both_versions_named() -> TestResult {
    let mut bytes = Vec::new();
    Envelope {
        pairing: pairing()?,
        request: Request::List,
    }
    .write(&mut bytes)?;
    bytes[0] = PROTOCOL_VERSION + 1;
    let refused = Envelope::read(&mut bytes.as_slice());
    let reason = refused.err().map(|error| error.to_string());
    assert!(reason.is_some_and(|reason| {
        reason.contains(&PROTOCOL_VERSION.to_string())
            && reason.contains(&(PROTOCOL_VERSION + 1).to_string())
    }));
    Ok(())
}

#[test]
fn a_path_escaping_id_is_refused_on_read() -> TestResult {
    let mut bytes = Vec::new();
    Envelope {
        pairing: pairing()?,
        request: Request::Stop {
            id: "abcd".parse()?,
        },
    }
    .write(&mut bytes)?;
    let at = bytes.len() - 4;
    bytes[at..].copy_from_slice(b"../x");
    assert!(Envelope::read(&mut bytes.as_slice()).is_err());
    Ok(())
}
