use krad_chat::{gateway_models, GatewayError};
use krad_domain::{GatewayAccess, GatewayKey};
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread::{self, JoinHandle};

const KEY: &str = "sk-local-test";
const MODELS: &str = r#"{"object":"list","data":[{"id":"gpt-5","object":"model","owned_by":"openai"},{"id":"gemini-3-pro","object":"model","owned_by":"google"}]}"#;
const OK_STATUS: &str = "200 OK";
const UNAUTHORIZED_STATUS: &str = "401 Unauthorized";
const ANY_LOOPBACK_PORT: &str = "127.0.0.1:0";
const HTTP_SCHEME: &str = "http://";

type Served = JoinHandle<io::Result<Vec<String>>>;

fn answer_once(listener: &TcpListener, status: &str, body: &str) -> io::Result<Vec<String>> {
    let (mut stream, _) = listener.accept()?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut head = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.trim().is_empty() {
            break;
        }
        head.push(line.trim().to_owned());
    }
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    Ok(head)
}

fn serve_once(status: &'static str, body: &'static str) -> io::Result<(String, Served)> {
    let listener = TcpListener::bind(ANY_LOOPBACK_PORT)?;
    let url = format!("{HTTP_SCHEME}{}", listener.local_addr()?);
    let server = thread::spawn(move || answer_once(&listener, status, body));
    Ok((url, server))
}

fn access(url: &str) -> GatewayAccess {
    GatewayAccess {
        url: url
            .parse()
            .unwrap_or_else(|_| unreachable!("{url} is a loopback address")),
        key: GatewayKey::parse(KEY).unwrap_or_else(|| unreachable!("the key is not blank")),
    }
}

fn served_head(server: Served) -> io::Result<Vec<String>> {
    server
        .join()
        .unwrap_or_else(|_| unreachable!("the test server does not panic"))
}

#[test]
fn the_model_list_is_asked_with_the_key_as_a_bearer_token() -> Result<(), Box<dyn std::error::Error>>
{
    let (url, server) = serve_once(OK_STATUS, MODELS)?;
    let models = gateway_models(&access(&url))?;
    let head = served_head(server)?;
    assert!(head[0].starts_with("GET /v1/models "), "{head:?}");
    assert!(head
        .iter()
        .any(|line| line.eq_ignore_ascii_case(&format!("authorization: Bearer {KEY}"))));
    let ids: Vec<&str> = models
        .iter()
        .map(|model| model.offer.value.as_str())
        .collect();
    assert_eq!(ids, ["gpt-5", "gemini-3-pro"]);
    Ok(())
}

#[test]
fn a_rejected_key_is_told_apart_and_never_shown() -> io::Result<()> {
    let (url, server) = serve_once(UNAUTHORIZED_STATUS, "{}")?;
    let answer = gateway_models(&access(&url));
    served_head(server)?;
    assert!(matches!(answer, Err(GatewayError::Rejected)));
    assert!(answer.is_err_and(|error| !error.to_string().contains(KEY)));
    Ok(())
}

#[test]
fn a_gateway_that_is_not_running_is_unreachable() -> io::Result<()> {
    let listener = TcpListener::bind(ANY_LOOPBACK_PORT)?;
    let url = format!("{HTTP_SCHEME}{}", listener.local_addr()?);
    drop(listener);
    let answer = gateway_models(&access(&url));
    assert!(
        matches!(answer, Err(GatewayError::Unreachable(_))),
        "{answer:?}"
    );
    Ok(())
}
