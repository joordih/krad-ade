#![cfg(unix)]

use krad_protocol::{read_frame, write_frame, Frame, Hello, Message};
use krad_runner::RuntimeServer;
use krad_runtime::{Runtime, RuntimeSetup};
use krad_web_listener::{bind_web_access, TlsFiles, WebAccess};
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::error::Error;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tungstenite::client::IntoClientRequest as _;
use tungstenite::http::HeaderValue;
use tungstenite::Message as WsMessage;

const FIXTURES: &str = "tests/fixtures";
const CERTIFICATE: &str = "localhost.crt";
const AUTHORITY: &str = "ca.crt";
const KEY: &str = "localhost.key";
const HOST: &str = "localhost";
const TIMEOUT: Duration = Duration::from_secs(10);

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURES)
        .join(name)
}

fn client_config() -> Result<Arc<ClientConfig>, Box<dyn Error>> {
    let mut roots = RootCertStore::empty();
    for certificate in CertificateDer::pem_file_iter(fixture(AUTHORITY))? {
        roots.add(certificate?)?;
    }
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots)
            .with_no_client_auth();
    Ok(Arc::new(config))
}

#[test]
fn a_browser_speaks_to_the_runtime_over_tls() -> Result<(), Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("krad-web-tls-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let server = RuntimeServer::new(Runtime::start(RuntimeSetup::default())?, &root);
    let access = WebAccess {
        bind: "127.0.0.1:0".parse()?,
        tls: Some(TlsFiles {
            certificate: fixture(CERTIFICATE),
            key: fixture(KEY),
        }),
        names: Vec::new(),
    };
    let listener = bind_web_access(&access, &server)?;
    let address = listener.address;
    let serving = Arc::clone(&server);
    thread::spawn(move || serving.serve(&listener));

    let tcp = TcpStream::connect(address)?;
    tcp.set_read_timeout(Some(TIMEOUT))?;
    let connection = ClientConnection::new(client_config()?, ServerName::try_from(HOST)?)?;
    let tls = StreamOwned::new(connection, tcp);
    let port = address.port();
    let mut request = format!("wss://{HOST}:{port}/runtime").into_client_request()?;
    request.headers_mut().insert(
        "Origin",
        HeaderValue::from_str(&format!("https://{HOST}:{port}"))?,
    );
    let (mut socket, answer) = tungstenite::client(request, tls)?;
    assert!(answer.headers().contains_key("strict-transport-security"));

    let mut bytes = Vec::new();
    write_frame(
        &mut bytes,
        &Frame::Message(Box::new(Message::Hello(Hello::new("web")))),
    )?;
    socket.send(WsMessage::Binary(bytes.split_off(4).into()))?;
    let welcome = loop {
        if let WsMessage::Binary(payload) = socket.read()? {
            let mut framed = u32::try_from(payload.len())?.to_be_bytes().to_vec();
            framed.extend_from_slice(&payload);
            break read_frame(&mut framed.as_slice())?;
        }
    };
    assert!(
        matches!(welcome, Some(Frame::Message(message)) if matches!(*message, Message::Welcome(_)))
    );
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
