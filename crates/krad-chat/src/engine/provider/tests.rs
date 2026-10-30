use super::*;
use crate::engine::attachments::Attachments;
use crate::engine::wire::{Call, Message, Outcome, Stop, ToolSpec, Usage};
use krad_domain::{ChatImage, GatewayAccess, GatewayKey, ImageFormat};
use serde_json::json;
use std::borrow::Cow;
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::thread::{self, JoinHandle};

const KEY: &str = "sk-engine-test";
const ANY_LOOPBACK_PORT: &str = "127.0.0.1:0";
const HTTP_SCHEME: &str = "http://";

pub(super) fn conversation(thinking: Protocol) -> Vec<Message> {
    vec![
        Message::user("fix it".to_owned()),
        Message::Assistant {
            blocks: vec![
                Block::Thinking {
                    text: "look first".to_owned(),
                    signature: Some("sig-1".to_owned()),
                    protocol: thinking,
                },
                Block::Text {
                    text: "Reading".to_owned(),
                },
                Block::Call(Call {
                    id: "c1".to_owned(),
                    name: "read".to_owned(),
                    arguments: json!({"path": "a.rs"}),
                }),
            ],
            usage: Usage::default(),
            stop: Stop::Tools,
        },
        Message::Results {
            outcomes: vec![Outcome {
                call: "c1".to_owned(),
                name: "read".to_owned(),
                content: "fn a() {}".to_owned(),
                failed: false,
            }],
        },
        Message::user("and the tests".to_owned()),
    ]
}

pub(super) fn read_tool() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: Cow::Borrowed("read"),
        description: Cow::Borrowed("Read a file"),
        parameters: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
    }]
}

pub(super) fn request<'a>(messages: &'a [Message], tools: &'a [ToolSpec]) -> Request<'a> {
    Request {
        model: "test-model",
        system: "be brief",
        messages,
        images: Attachments::none(),
        tools,
        native: None,
        effort: None,
        max_tokens: 1_000,
    }
}

pub(super) fn pictured() -> (Vec<Message>, Attachments) {
    let dir = std::env::temp_dir()
        .join("krad-provider-images")
        .join(crate::live::new_session_id().as_str());
    std::fs::create_dir_all(&dir).unwrap_or_default();
    let shot = dir.join("shot.png");
    std::fs::write(&shot, b"png").unwrap_or_default();
    let image = |path: std::path::PathBuf| ChatImage {
        path,
        format: ImageFormat::Png,
    };
    let messages = vec![Message::User {
        text: "what is this?".to_owned(),
        images: vec![image(shot), image("/nowhere/gone.png".into())],
    }];
    let attachments = Attachments::read(&messages);
    (messages, attachments)
}

pub(super) const GONE_NOTE: &str = "[The image /nowhere/gone.png could not be read]";

pub(super) fn parse<P: Parse>(
    parser: P,
    stream: &str,
) -> (Result<Reply, ProviderError>, Vec<Delta>) {
    let mut deltas = Vec::new();
    let reply = read_stream(
        io::Cursor::new(stream.to_owned()),
        parser,
        &AtomicBool::new(false),
        &mut |delta| deltas.push(delta),
    );
    (reply, deltas)
}

struct Served {
    head: Vec<String>,
}

fn read_head(stream: &mut impl Read) -> io::Result<Vec<String>> {
    let mut bytes = Vec::new();
    let mut byte = [0_u8; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte)? == 0 {
            break;
        }
        bytes.extend_from_slice(&byte);
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn serve_once(
    status: &'static str,
    body: &'static str,
) -> io::Result<(String, JoinHandle<io::Result<Served>>)> {
    let listener = TcpListener::bind(ANY_LOOPBACK_PORT)?;
    let url = format!("{HTTP_SCHEME}{}", listener.local_addr()?);
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept()?;
        let head = read_head(&mut stream)?;
        let length = head
            .iter()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or_default();
        let mut sent = vec![0_u8; length];
        stream.read_exact(&mut sent)?;
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}"
        )?;
        Ok(Served { head })
    });
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

fn joined(server: JoinHandle<io::Result<Served>>) -> io::Result<Served> {
    server
        .join()
        .unwrap_or_else(|_| unreachable!("the test server does not panic"))
}

fn ask(url: &str, protocol: Protocol) -> (Result<Reply, ProviderError>, Vec<Delta>) {
    let messages = [Message::user("hi".to_owned())];
    let mut deltas = Vec::new();
    let reply = stream(
        &access(url),
        protocol,
        &request(&messages, &[]),
        &AtomicBool::new(false),
        &mut |delta| deltas.push(delta),
    );
    (reply, deltas)
}

#[test]
fn each_protocol_posts_to_its_path_with_the_key_and_streams_the_answer() -> io::Result<()> {
    let answers = [
        (
            Protocol::Messages,
            "POST /v1/messages ",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"pong\"}}\n\n",
        ),
        (
            Protocol::Responses,
            "POST /v1/responses ",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"pong\"}\n\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"pong\"}]}}\n\n",
        ),
        (
            Protocol::Completions,
            "POST /v1/chat/completions ",
            "data: {\"choices\":[{\"delta\":{\"content\":\"pong\"}}]}\n\ndata: [DONE]\n\n",
        ),
    ];
    for (protocol, path, body) in answers {
        let (url, server) = serve_once("200 OK", body)?;
        let (reply, deltas) = ask(&url, protocol);
        let received = joined(server)?;
        assert!(received.head[0].starts_with(path), "{:?}", received.head);
        assert!(received
            .head
            .iter()
            .any(|line| line.eq_ignore_ascii_case(&format!("authorization: Bearer {KEY}"))));
        assert_eq!(
            received
                .head
                .iter()
                .any(|line| line.eq_ignore_ascii_case("anthropic-version: 2023-06-01")),
            protocol == Protocol::Messages
        );
        assert_eq!(deltas, [Delta::Text("pong".to_owned())], "{protocol:?}");
        assert!(
            reply.is_ok_and(|reply| reply.blocks
                == [Block::Text {
                    text: "pong".to_owned()
                }]),
            "{protocol:?}"
        );
    }
    Ok(())
}

#[test]
fn a_rejected_key_and_a_failing_gateway_are_told_apart() -> io::Result<()> {
    let (url, server) = serve_once("401 Unauthorized", "{}")?;
    let (rejected, _) = ask(&url, Protocol::Completions);
    joined(server)?;
    assert!(matches!(rejected, Err(ProviderError::Rejected)));
    assert!(rejected.is_err_and(|error| !error.to_string().contains(KEY)));
    let (url, server) = serve_once(
        "403 Forbidden",
        "{\"error\":{\"message\":\"client_attestation_denied\"}}",
    )?;
    let (upstream, _) = ask(&url, Protocol::Completions);
    joined(server)?;
    assert!(
        matches!(&upstream, Err(ProviderError::Status { code: 403, detail }) if detail == "client_attestation_denied"),
        "an account behind the Gateway refusing is not the key: {upstream:?}"
    );
    let (url, server) = serve_once(
        "503 Service Unavailable",
        "{\"error\":{\"message\":\"no account left\"}}",
    )?;
    let (failed, _) = ask(&url, Protocol::Messages);
    joined(server)?;
    assert!(
        matches!(&failed, Err(ProviderError::Status { code: 503, detail }) if detail == "no account left"),
        "{failed:?}"
    );
    Ok(())
}

#[test]
fn a_gateway_that_is_not_running_is_unreachable() -> io::Result<()> {
    let listener = TcpListener::bind(ANY_LOOPBACK_PORT)?;
    let url = format!("{HTTP_SCHEME}{}", listener.local_addr()?);
    drop(listener);
    let (reply, _) = ask(&url, Protocol::Responses);
    assert!(
        matches!(reply, Err(ProviderError::Unreachable(_))),
        "{reply:?}"
    );
    Ok(())
}

#[test]
fn a_stopped_answer_ends_as_cancelled() {
    let (reply, _) = {
        let mut deltas = Vec::new();
        let reply = read_stream(
            io::Cursor::new("data: {\"choices\":[]}\n\n".to_owned()),
            completions::Parser::default(),
            &AtomicBool::new(true),
            &mut |delta| deltas.push(delta),
        );
        (reply, deltas)
    };
    assert!(matches!(reply, Err(ProviderError::Cancelled)));
}

#[test]
fn the_protocol_follows_the_owner_then_the_model_name() {
    assert_eq!(protocol_for(Some("anthropic"), "x"), Protocol::Messages);
    assert_eq!(protocol_for(Some("OpenAI"), "x"), Protocol::Responses);
    assert_eq!(protocol_for(Some("codex"), "x"), Protocol::Responses);
    assert_eq!(
        protocol_for(Some("google"), "claude-x"),
        Protocol::Completions
    );
    assert_eq!(protocol_for(None, "claude-sonnet-5-5"), Protocol::Messages);
    assert_eq!(protocol_for(Some(" "), "gpt-6-astra"), Protocol::Responses);
    assert_eq!(protocol_for(None, "gemini-3-pro"), Protocol::Completions);
}

#[test]
fn error_details_prefer_the_provider_message_and_stay_short() {
    assert_eq!(
        detail_of("{\"error\":{\"message\":\"quota\"}}"),
        "quota".to_owned()
    );
    assert_eq!(detail_of("{\"error\":\"plain\"}"), "plain".to_owned());
    assert_eq!(detail_of("  oops \n"), "oops".to_owned());
    assert_eq!(detail_of(&"x".repeat(DETAIL_CHARS * 2)).len(), DETAIL_CHARS);
}
