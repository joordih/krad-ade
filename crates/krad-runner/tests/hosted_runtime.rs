#![cfg(unix)]

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_agents::{call, Caller, Request, Response, TerminalHandle};
use krad_client::{ClientError, Cursor, Feeds, Runner, RuntimeClient};
use krad_domain::{InstallId, Pairing, PairingPermission, PairingPermissions, ToolDecision};
use krad_protocol::{
    BusCommand, Command, ErrorCode, EventKind, Feed, Hello, PairingAnswer, PairingCommand,
    PendingAsk, ProtocolVersion, Reply,
};
use krad_runner::{bind_runtime_socket, pair_over_ssh, RuntimeServer};
use krad_runtime::{BusSetup, Runtime, RuntimeHandle, RuntimeSetup};
use std::error::Error;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread::{self, JoinHandle};

type TestResult = Result<(), Box<dyn Error>>;

const BUILD: &str = "0.0.0-test";
const ENVIRONMENT: &str = "runner";
const CALLER: u64 = 1;
const ASKED_TOOL: &str = "Bash";
const FIRST_INSTALL: &str = "0123456789abcdef";
const SECOND_INSTALL: &str = "fedcba9876543210";
const PHONE_INSTALL: &str = "aaaabbbbccccdddd";
const NO_BUS: &str = "the runner's runtime serves no bus";
const FEED_ENDED: &str = "the feed ended";

struct Hosted {
    root: PathBuf,
    runtime: RuntimeHandle,
}

impl Drop for Hosted {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn hosted(name: &str) -> Result<Hosted, Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("krad-hosted-{name}-{}", std::process::id()));
    let runtime = Runtime::start(RuntimeSetup {
        bus: Some(BusSetup::default()),
        ..RuntimeSetup::default()
    })?;
    let listener = bind_runtime_socket(&root)?;
    let server = RuntimeServer::new(runtime.clone(), &root);
    thread::spawn(move || server.serve(&listener));
    Ok(Hosted { root, runtime })
}

impl Hosted {
    fn pair(&self, install: &str) -> Result<Pairing, Box<dyn Error>> {
        let install: InstallId = install.parse()?;
        let token = pair_over_ssh(&self.root, &install, "laptop".parse()?)?;
        Ok(Pairing { install, token })
    }

    fn connect(&self, hello: Hello) -> Result<Runner, ClientError> {
        let socket = krad_host::runner::runtime_socket(&self.root);
        let stream = UnixStream::connect(socket).map_err(|_| ClientError::Stopped)?;
        let reading = stream.try_clone().map_err(|_| ClientError::Stopped)?;
        Runner::connect(reading, stream, hello)
    }

    fn paired(&self, pairing: Pairing) -> Result<Runner, ClientError> {
        self.connect(Hello::paired(BUILD, pairing))
    }

    fn ask_in_background(&self) -> Result<JoinHandle<Option<Response>>, Box<dyn Error>> {
        let endpoint = &self.runtime.bus().ok_or(NO_BUS)?.endpoint;
        let address = endpoint.address.to_string();
        let token = endpoint.token.clone();
        let caller = Caller::Terminal(TerminalHandle {
            environment: ENVIRONMENT.to_owned(),
            terminal: CALLER,
        });
        let request = Request::Ask {
            tool: ASKED_TOOL.to_owned(),
            input: serde_json::json!({ "command": "rm -rf build" }),
            suggestions: Vec::new(),
            call: None,
        };
        Ok(thread::spawn(move || {
            call(&address, &token, &caller, request).ok()
        }))
    }
}

fn following(client: &Runner) -> Result<Feeds, Box<dyn Error>> {
    let mut feeds = client.subscribe(None);
    match block_on(feeds.next()).ok_or(FEED_ENDED)? {
        Feed::Snapshot(_) => Ok(feeds),
        Feed::Event(_) => Err("the feed did not start with a snapshot".into()),
    }
}

fn next_ask(feeds: &mut Feeds) -> Result<PendingAsk, Box<dyn Error>> {
    loop {
        if let Feed::Event(event) = block_on(feeds.next()).ok_or(FEED_ENDED)? {
            if let EventKind::AskPending(pending) = event.kind {
                return Ok(pending);
            }
        }
    }
}

fn answer(
    client: &Runner,
    pending: &PendingAsk,
    decision: ToolDecision,
) -> Result<Reply, ClientError> {
    block_on(client.send(Command::Bus(BusCommand::AnswerAsk {
        ask: pending.ask,
        decision,
    })))
}

fn allow() -> ToolDecision {
    ToolDecision::Allow {
        updated_input: None,
        permissions: Vec::new(),
    }
}

#[test]
fn two_clients_of_one_runner_see_the_same_ask_and_only_one_answers_it() -> TestResult {
    let hosted = hosted("two")?;
    let first = hosted.paired(hosted.pair(FIRST_INSTALL)?)?;
    let second = hosted.paired(hosted.pair(SECOND_INSTALL)?)?;
    assert_eq!(first.welcome().permissions, Some(PairingPermissions::all()));
    assert_ne!(first.next_request(), second.next_request());
    let mut first_feed = following(&first)?;
    let mut second_feed = following(&second)?;
    let asked = hosted.ask_in_background()?;
    let seen_first = next_ask(&mut first_feed)?;
    let seen_second = next_ask(&mut second_feed)?;
    assert_eq!(seen_first, seen_second);
    assert_eq!(answer(&first, &seen_first, allow())?, Reply::Accepted);
    let late = ToolDecision::Deny {
        message: "late".to_owned(),
        interrupt: false,
    };
    assert!(matches!(
        answer(&second, &seen_second, late)?,
        Reply::Refused {
            code: ErrorCode::AlreadyAnswered,
            ..
        }
    ));
    assert_eq!(
        asked.join().map_err(|_| "the caller panicked")?,
        Some(Response::Decided(allow()))
    );
    Ok(())
}

#[test]
fn a_second_client_pairs_with_a_code_instead_of_ssh_and_can_be_revoked() -> TestResult {
    let hosted = hosted("code")?;
    let admin = hosted.paired(hosted.pair(FIRST_INSTALL)?)?;
    let read: PairingPermissions = [PairingPermission::Read].into_iter().collect();
    let PairingAnswer::Code { code, .. } =
        block_on(admin.pairing(PairingCommand::CreatePairingCode {
            permissions: read.clone(),
        }))?
    else {
        return Err("no pairing code".into());
    };
    let newcomer = hosted.connect(Hello::new(BUILD))?;
    assert_eq!(
        newcomer.welcome().permissions,
        Some(PairingPermissions::default())
    );
    assert!(matches!(
        block_on(newcomer.pairing(PairingCommand::ListPairings)),
        Err(ClientError::Refused {
            code: ErrorCode::Forbidden,
            ..
        })
    ));
    let install: InstallId = PHONE_INSTALL.parse()?;
    let redeem = PairingCommand::RedeemPairingCode {
        code,
        install: install.clone(),
        device: "phone".parse()?,
    };
    let PairingAnswer::Redeemed { token, permissions } =
        block_on(newcomer.pairing(redeem.clone()))?
    else {
        return Err("the code was not redeemed".into());
    };
    assert_eq!(permissions, read);
    assert!(
        block_on(newcomer.pairing(redeem)).is_err(),
        "a code is used once"
    );
    let phone_pairing = Pairing {
        install: install.clone(),
        token,
    };
    let phone = hosted.paired(phone_pairing.clone())?;
    assert_eq!(phone.welcome().permissions, Some(read));
    let mut phone_feed = following(&phone)?;
    let asked = hosted.ask_in_background()?;
    let pending = next_ask(&mut phone_feed)?;
    assert!(matches!(
        answer(&phone, &pending, allow())?,
        Reply::Refused {
            code: ErrorCode::Forbidden,
            ..
        }
    ));
    assert_eq!(answer(&admin, &pending, allow())?, Reply::Accepted);
    drop(asked.join());
    let PairingAnswer::Pairings { devices } =
        block_on(admin.pairing(PairingCommand::ListPairings))?
    else {
        return Err("no pairings listed".into());
    };
    let phone_device = devices
        .iter()
        .find(|device| device.install == install)
        .ok_or("the phone is not listed")?;
    assert_eq!(phone_device.device.as_str(), "phone");
    assert_eq!(
        block_on(admin.pairing(PairingCommand::RevokePairing {
            install: install.clone()
        }))?,
        PairingAnswer::Revoked
    );
    while block_on(phone_feed.next()).is_some() {}
    assert!(matches!(
        hosted.paired(phone_pairing),
        Err(ClientError::Refused {
            code: ErrorCode::Forbidden,
            ..
        })
    ));
    Ok(())
}

#[test]
fn a_client_of_another_major_version_is_refused_as_unsupported() -> TestResult {
    let hosted = hosted("version")?;
    let mut hello = Hello::paired(BUILD, hosted.pair(FIRST_INSTALL)?);
    hello.protocol = ProtocolVersion(ProtocolVersion::CURRENT.0 + 1);
    assert!(matches!(
        hosted.connect(hello),
        Err(ClientError::Refused {
            code: ErrorCode::Unsupported,
            ..
        })
    ));
    let client = hosted.paired(hosted.pair(FIRST_INSTALL)?)?;
    assert!(matches!(
        block_on(client.send(Command::Unknown))?,
        Reply::Refused {
            code: ErrorCode::Unsupported,
            ..
        }
    ));
    Ok(())
}

#[test]
fn a_desktop_back_on_the_same_runner_run_resumes_from_its_seq() -> TestResult {
    let hosted = hosted("resume")?;
    let pairing = hosted.pair(FIRST_INSTALL)?;
    let away = hosted.paired(pairing.clone())?;
    let mut cursor = Cursor::default();
    assert_eq!(cursor.welcomed(away.welcome()), None);
    let mut feed = away.subscribe(None);
    let snapshot = block_on(feed.next()).ok_or(FEED_ENDED)?;
    assert!(matches!(snapshot, Feed::Snapshot(_)));
    cursor.heard(&snapshot);
    drop(feed);
    drop(away);
    let asked = hosted.ask_in_background()?;
    let back = hosted.paired(pairing)?;
    let since = cursor.welcomed(back.welcome());
    assert!(since.is_some(), "the runner's runtime kept its run");
    let mut resumed = back.subscribe(since);
    let first = block_on(resumed.next()).ok_or(FEED_ENDED)?;
    let Feed::Event(first) = first else {
        return Err("a client back within the ring got a snapshot".into());
    };
    let pending = match first.kind {
        EventKind::AskPending(pending) => pending,
        _ => next_ask(&mut resumed)?,
    };
    assert_eq!(answer(&back, &pending, allow())?, Reply::Accepted);
    assert_eq!(
        asked.join().map_err(|_| "the caller panicked")?,
        Some(Response::Decided(allow()))
    );
    Ok(())
}

#[test]
fn a_remote_client_answers_the_bus_requests_the_runtime_delegates() -> TestResult {
    let hosted = hosted("delegations")?;
    let client = hosted.paired(hosted.pair(FIRST_INSTALL)?)?;
    let mut delegated = client.take_delegations();
    let endpoint = &hosted.runtime.bus().ok_or(NO_BUS)?.endpoint;
    let (address, token) = (endpoint.address.to_string(), endpoint.token.clone());
    let opened = std::path::PathBuf::from("/runner/repos/api/README.md");
    let wanted = opened.clone();
    let asking = thread::spawn(move || {
        let request = Request::OpenFile {
            path: wanted,
            position: None,
        };
        call(&address, &token, &Caller::Client, request).ok()
    });
    let request = block_on(delegated.next()).ok_or("nothing was delegated")?;
    assert_eq!(request.from, Caller::Client);
    assert!(matches!(&request.request, Request::OpenFile { path, .. } if *path == opened));
    let answered = block_on(client.send(Command::Bus(BusCommand::Respond {
        delegation: request.delegation,
        response: Response::Noted,
    })))?;
    assert_eq!(answered, Reply::Accepted);
    assert_eq!(
        asking.join().map_err(|_| "the caller panicked")?,
        Some(Response::Noted)
    );
    let again = block_on(client.send(Command::Bus(BusCommand::Respond {
        delegation: request.delegation,
        response: Response::Noted,
    })))?;
    assert!(matches!(
        again,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    Ok(())
}
