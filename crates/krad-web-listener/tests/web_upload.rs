#![cfg(unix)]

use krad_domain::{EnvironmentData, EnvironmentName, RuntimeState};
use krad_runner::{pair_over_ssh, RuntimeServer};
use krad_runtime::{EnvironmentsSetup, Runtime, RuntimeSetup};
use krad_web_listener::{bind_web_access, WebAccess};
use std::error::Error;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const BROWSER: &str = "web-browser-0002";
const TIMEOUT: Duration = Duration::from_secs(10);

struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn http(address: SocketAddr, head: &str, body: &[u8]) -> Result<String, Box<dyn Error>> {
    let mut socket = TcpStream::connect(address)?;
    socket.set_read_timeout(Some(TIMEOUT))?;
    socket.write_all(head.as_bytes())?;
    socket.write_all(body)?;
    let mut answer = String::new();
    socket.read_to_string(&mut answer)?;
    Ok(answer)
}

#[test]
fn a_paired_browser_uploads_a_file_into_a_folder_of_the_environment() -> TestResult {
    let folder =
        Folder(std::env::temp_dir().join(format!("krad-web-upload-{}", std::process::id())));
    let files = folder.0.join("app");
    let root = folder.0.join("runtime");
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&root)?;
    let mut data = EnvironmentData::empty(EnvironmentName::new(ENVIRONMENT.to_owned()));
    data.add_group(files.clone(), None);
    let runtime = Runtime::start(RuntimeSetup {
        environments: EnvironmentsSetup {
            state: RuntimeState {
                environments: vec![data],
                ..RuntimeState::default()
            },
            file: None,
        },
        ..RuntimeSetup::default()
    })?;
    let server = RuntimeServer::new(runtime, &root);
    let listener = bind_web_access(
        &WebAccess {
            bind: "127.0.0.1:0".parse()?,
            tls: None,
            names: Vec::new(),
        },
        &server,
    )?;
    let address = listener.address;
    let serving = Arc::clone(&server);
    thread::spawn(move || serving.serve(&listener));

    let token = pair_over_ssh(&root, &BROWSER.parse()?, "Browser".parse()?)?;
    let cookie = format!("krad-pairing={BROWSER}.{}", token.expose());
    let body = b"\x89PNG not really";
    let query = format!(
        "/upload?environment={ENVIRONMENT}&dir={}&name=logo%20one.png",
        files.display().to_string().replace('/', "%2F")
    );
    let head = |cookie: &str, origin: &str| {
        format!(
            "POST {query} HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nCookie: {cookie}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
    };
    let origin = format!("http://{address}");
    let refused = http(address, &head("theme=dark", &origin), body)?;
    assert!(refused.starts_with("HTTP/1.1 403"), "{refused}");
    let cross = http(address, &head(&cookie, "http://evil.example"), body)?;
    assert!(cross.starts_with("HTTP/1.1 403"), "{cross}");
    let answer = http(address, &head(&cookie, &origin), body)?;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert_eq!(std::fs::read(files.join("logo one.png"))?, body);
    let again = http(address, &head(&cookie, &origin), body)?;
    assert!(again.starts_with("HTTP/1.1 400"), "{again}");
    Ok(())
}
