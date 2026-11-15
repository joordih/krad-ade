#![cfg(unix)]

use krad_domain::{InstallId, Pairing, PairingPermission, PairingPermissions};
use krad_protocol::{
    read_frame, write_frame, Feed, Frame, Hello, Message, PairingAnswer, PairingCommand, RequestId,
};
use krad_runner::{pair_over_ssh, RuntimeServer};
use krad_runtime::{Runtime, RuntimeSetup};
use krad_web_listener::{bind_web_access, WebAccess};
use std::error::Error;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tungstenite::client::IntoClientRequest as _;
use tungstenite::http::HeaderValue;
use tungstenite::{Message as WsMessage, WebSocket};

type TestResult = Result<(), Box<dyn Error>>;

const BUILD: &str = "0.0.0-test";
const ADMIN_INSTALL: &str = "desktop-0123456789";
const BROWSER_INSTALL: &str = "web-browser-0001";
const TIMEOUT: Duration = Duration::from_secs(10);

struct Served {
    root: PathBuf,
    address: SocketAddr,
}

impl Drop for Served {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn served(name: &str) -> Result<Served, Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("krad-web-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let server = RuntimeServer::new(runtime, &root);
    let access = WebAccess {
        bind: "127.0.0.1:0".parse()?,
        tls: None,
        names: Vec::new(),
    };
    let listener = bind_web_access(&access, &server)?;
    let address = listener.address;
    let serving = Arc::clone(&server);
    thread::spawn(move || serving.serve(&listener));
    Ok(Served { root, address })
}

impl Served {
    fn origin(&self) -> String {
        format!("http://{}", self.address)
    }

    fn http(&self, request: &str) -> Result<String, Box<dyn Error>> {
        let mut socket = TcpStream::connect(self.address)?;
        socket.set_read_timeout(Some(TIMEOUT))?;
        socket.write_all(request.as_bytes())?;
        let mut answer = String::new();
        socket.read_to_string(&mut answer)?;
        Ok(answer)
    }

    fn socket(
        &self,
        origin: &str,
        cookie: Option<&str>,
    ) -> Result<WebSocket<TcpStream>, Box<dyn Error>> {
        let mut request = format!("ws://{}/runtime", self.address).into_client_request()?;
        request
            .headers_mut()
            .insert("Origin", HeaderValue::from_str(origin)?);
        if let Some(cookie) = cookie {
            request
                .headers_mut()
                .insert("Cookie", HeaderValue::from_str(cookie)?);
        }
        let stream = TcpStream::connect(self.address)?;
        stream.set_read_timeout(Some(TIMEOUT))?;
        let (socket, _) = tungstenite::client(request, stream)?;
        Ok(socket)
    }
}

fn send(socket: &mut WebSocket<TcpStream>, message: Message) -> TestResult {
    let mut bytes = Vec::new();
    write_frame(&mut bytes, &Frame::Message(Box::new(message)))?;
    socket.send(WsMessage::Binary(bytes.split_off(4).into()))?;
    Ok(())
}

fn receive(socket: &mut WebSocket<TcpStream>) -> Result<Message, Box<dyn Error>> {
    loop {
        if let WsMessage::Binary(payload) = socket.read()? {
            let mut bytes = u32::try_from(payload.len())?.to_be_bytes().to_vec();
            bytes.extend_from_slice(&payload);
            if let Some(Frame::Message(message)) = read_frame(&mut bytes.as_slice())? {
                return Ok(*message);
            }
        }
    }
}

fn welcomed(socket: &mut WebSocket<TcpStream>, hello: Hello) -> Result<Message, Box<dyn Error>> {
    send(socket, Message::Hello(hello))?;
    receive(socket)
}

fn browser_permissions() -> PairingPermissions {
    [
        PairingPermission::Read,
        PairingPermission::Answer,
        PairingPermission::Drive,
    ]
    .into_iter()
    .collect()
}

fn cookie_of(answer: &str) -> Result<String, Box<dyn Error>> {
    let line = answer
        .lines()
        .find(|line| line.starts_with("Set-Cookie: "))
        .ok_or("no cookie was set")?;
    let value = line
        .trim_start_matches("Set-Cookie: ")
        .split(';')
        .next()
        .ok_or("an empty cookie")?;
    Ok(value.to_owned())
}

#[test]
fn the_page_is_served_with_its_security_headers() -> TestResult {
    let served = served("page")?;
    let answer = served.http(&format!(
        "GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        served.address
    ))?;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.contains("Content-Security-Policy: default-src 'self'"));
    assert!(answer.contains("<title>Krad Web</title>"));
    let rebound =
        served.http("GET / HTTP/1.1\r\nHost: evil.example\r\nConnection: close\r\n\r\n")?;
    assert!(rebound.starts_with("HTTP/1.1 403"), "{rebound}");
    Ok(())
}

#[test]
fn the_theme_is_served_as_css_from_the_runtime_settings() -> TestResult {
    let served = served("theme")?;
    let answer = served.http(&format!(
        "GET /theme.css HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        served.address
    ))?;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.contains("Content-Type: text/css"));
    assert!(answer.contains("--krad-accent: #"));
    assert!(answer.contains("--krad-text-sm: 13px;"));
    Ok(())
}

#[test]
fn a_socket_from_another_origin_is_refused() -> TestResult {
    let served = served("origin")?;
    assert!(served.socket("http://evil.example", None).is_err());
    Ok(())
}

#[test]
fn a_browser_pairs_with_a_code_and_follows_the_runtime_through_its_cookie() -> TestResult {
    let served = served("pairing")?;
    let admin = Pairing {
        install: ADMIN_INSTALL.parse()?,
        token: pair_over_ssh(&served.root, &ADMIN_INSTALL.parse()?, "laptop".parse()?)?,
    };
    let mut desktop = served.socket(&served.origin(), None)?;
    assert!(matches!(
        welcomed(&mut desktop, Hello::paired(BUILD, admin))?,
        Message::Welcome(_)
    ));
    send(
        &mut desktop,
        Message::Pairing {
            id: RequestId(1),
            command: PairingCommand::CreatePairingCode {
                permissions: browser_permissions(),
            },
        },
    )?;
    let Message::Paired {
        answer: PairingAnswer::Code { code, .. },
        ..
    } = receive(&mut desktop)?
    else {
        return Err("no pairing code".into());
    };

    let body = format!(
        r#"{{"code":"{}","install":"{BROWSER_INSTALL}","device":"Browser"}}"#,
        code.expose()
    );
    let cross = served.http(&format!(
        "POST /pairing HTTP/1.1\r\nHost: {}\r\nOrigin: http://evil.example\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        served.address,
        body.len()
    ))?;
    assert!(cross.starts_with("HTTP/1.1 403"), "{cross}");
    let answer = served.http(&format!(
        "POST /pairing HTTP/1.1\r\nHost: {}\r\nOrigin: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        served.address,
        served.origin(),
        body.len()
    ))?;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.contains("HttpOnly"));
    let cookie = cookie_of(&answer)?;

    let mut browser = served.socket(&served.origin(), Some(&cookie))?;
    let Message::Welcome(welcome) = welcomed(&mut browser, Hello::new(BUILD))? else {
        return Err("the browser was not welcomed".into());
    };
    assert_eq!(welcome.permissions, Some(browser_permissions()));
    send(&mut browser, Message::Subscribe { since: None })?;
    assert!(matches!(
        receive(&mut browser)?,
        Message::Feed(Feed::Snapshot(_))
    ));

    let install: InstallId = BROWSER_INSTALL.parse()?;
    send(
        &mut desktop,
        Message::Pairing {
            id: RequestId(2),
            command: PairingCommand::RevokePairing { install },
        },
    )?;
    assert!(matches!(
        receive(&mut desktop)?,
        Message::Paired {
            answer: PairingAnswer::Revoked,
            ..
        }
    ));
    let ended = loop {
        match browser.read() {
            Ok(WsMessage::Close(_)) | Err(_) => break true,
            Ok(_) => {}
        }
    };
    assert!(ended);
    Ok(())
}
