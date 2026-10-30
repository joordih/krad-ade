use super::*;
use krad_domain::{McpServer, McpServerState, McpUrl};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

fn name(text: &str) -> McpServerName {
    text.parse()
        .unwrap_or_else(|_| unreachable!("{text} is a server name"))
}

fn variable(text: &str) -> McpVariableName {
    text.parse()
        .unwrap_or_else(|_| unreachable!("{text} is a variable name"))
}

fn secret(text: &str) -> McpSecret {
    McpSecret::parse(text).unwrap_or_else(|| unreachable!("not blank"))
}

#[test]
fn tool_names_are_prefixed_cleaned_capped_and_unique() {
    let mut taken = HashSet::new();
    assert_eq!(
        tool_name("my.server", "read file", &taken),
        "mcp__my_server__read_file"
    );
    let long = "x".repeat(100);
    let first = tool_name("s", &long, &taken);
    assert_eq!(first.len(), MAX_TOOL_NAME);
    taken.insert(first.clone());
    let second = tool_name("s", &long, &taken);
    assert_eq!(second.len(), MAX_TOOL_NAME);
    assert_ne!(second, first);
    assert!(second.ends_with("_2"));
}

#[test]
fn an_mcp_tool_name_reads_as_its_server_and_tool() {
    assert!(is_mcp_tool("mcp__files__read_file"));
    assert!(!is_mcp_tool("read"));
    assert_eq!(shown_tool("mcp__files__read_file"), "files · read_file");
    assert_eq!(shown_tool("mcp__lonely"), "lonely");
}

#[cfg(unix)]
mod stdio_server {
    use super::*;

    const FAKE_SERVER: &str = r#"
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\),.*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1"}}}\n' "$id" ;;
    *'"cursor":"page2"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"fail"}]}}\n' "$id" ;;
    *'"method":"tools/list"'*)
      printf '{"jsonrpc":"2.0","method":"notifications/message","params":{}}\n'
      printf '{"jsonrpc":"2.0","id":"s1","method":"ping"}\n'
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"greet","description":"Says hello","inputSchema":{"type":"object","properties":{"who":{"type":"string"}}}}],"nextCursor":"page2"}}\n' "$id" ;;
    *'"name":"greet"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"hello %s %s"}]}}\n' "$id" "$GREETING" "$PLAIN" ;;
    *'"name":"fail"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"it broke"}],"isError":true}}\n' "$id" ;;
  esac
done
"#;

    fn stdio(name_text: &str, script: &str, env: Vec<McpVariable>) -> McpServer {
        McpServer {
            name: name(name_text),
            transport: McpTransport::Stdio {
                command: "sh".to_owned(),
                args: vec!["-c".to_owned(), script.to_owned()],
                env,
            },
            state: McpServerState::Enabled,
        }
    }

    #[test]
    fn a_stdio_server_lists_every_page_and_answers_calls() {
        let server = stdio(
            "fake",
            FAKE_SERVER,
            vec![
                McpVariable {
                    name: variable("GREETING"),
                    value: McpValue::Secret,
                },
                McpVariable {
                    name: variable("PLAIN"),
                    value: McpValue::Plain("${KRAD_MCP_UNSET_VARIABLE:-friend}".to_owned()),
                },
            ],
        );
        let start = McpServerStart {
            server,
            secrets: HashMap::from([(variable("GREETING"), secret("world"))]),
        };
        let (toolbox, failures) = McpToolbox::start(&[start], Path::new("."));
        assert!(failures.is_empty(), "{failures:?}");
        let names: Vec<&str> = toolbox
            .tools()
            .iter()
            .map(|tool| tool.name.as_str())
            .collect();
        assert_eq!(names, ["mcp__fake__greet", "mcp__fake__fail"]);
        assert_eq!(toolbox.tools()[0].description, "Says hello");
        assert_eq!(toolbox.tools()[1].parameters["type"], "object");
        assert_eq!(
            toolbox
                .call("mcp__fake__greet", json!({"who": "me"}))
                .ok()
                .as_deref(),
            Some("hello world friend")
        );
        assert!(matches!(
            toolbox.call("mcp__fake__fail", Value::Null),
            Err(McpError::ToolFailed(text)) if text == "it broke"
        ));
        assert!(matches!(
            toolbox.call("mcp__fake__missing", Value::Null),
            Err(McpError::UnknownTool(_))
        ));
    }

    #[test]
    fn a_server_that_dies_or_lacks_a_secret_is_reported_and_others_still_start() {
        let dying = McpServerStart {
            server: stdio("dying", "echo boom >&2; exit 1", Vec::new()),
            secrets: HashMap::new(),
        };
        let locked = McpServerStart {
            server: stdio(
                "locked",
                FAKE_SERVER,
                vec![McpVariable {
                    name: variable("TOKEN"),
                    value: McpValue::Secret,
                }],
            ),
            secrets: HashMap::new(),
        };
        let fine = McpServerStart {
            server: stdio("fine", FAKE_SERVER, Vec::new()),
            secrets: HashMap::new(),
        };
        let (toolbox, failures) = McpToolbox::start(&[dying, locked, fine], Path::new("."));
        assert_eq!(toolbox.tools().len(), 2);
        assert_eq!(failures.len(), 2);
        assert!(
            matches!(&failures[0].error, McpError::Exited { stderr } if stderr == "boom"),
            "{:?}",
            failures[0].error
        );
        assert!(matches!(
            &failures[1].error,
            McpError::MissingSecret(name) if name.as_str() == "TOKEN"
        ));
    }
}

struct Exchange {
    headers: Vec<String>,
    body: String,
}

fn read_exchange(stream: &mut TcpStream) -> Option<Exchange> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut headers = Vec::new();
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end().to_owned();
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().ok()?;
            }
        }
        headers.push(line.to_ascii_lowercase());
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Exchange {
        headers,
        body: String::from_utf8(body).ok()?,
    })
}

fn respond(stream: &mut TcpStream, status: &str, extra: &str, kind: &str, body: &str) {
    let reply = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    );
    stream.write_all(reply.as_bytes()).unwrap_or_default();
}

fn request_id(body: &str) -> u64 {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value["id"].as_u64())
        .unwrap_or_default()
}

fn serve(mut stream: TcpStream) {
    let Some(exchange) = read_exchange(&mut stream) else {
        return;
    };
    let id = request_id(&exchange.body);
    let has = |header: &str| exchange.headers.iter().any(|line| line == header);
    if !has("authorization: bearer token-1") {
        return respond(&mut stream, "401 Unauthorized", "", "text/plain", "");
    }
    if exchange.body.contains("\"initialize\"") {
        let body = format!(
            r#"{{"jsonrpc":"2.0","id":{id},"result":{{"protocolVersion":"2025-06-18","capabilities":{{}}}}}}"#
        );
        return respond(
            &mut stream,
            "200 OK",
            "Mcp-Session-Id: session-7\r\n",
            "application/json",
            &body,
        );
    }
    if !has("mcp-session-id: session-7") {
        return respond(
            &mut stream,
            "400 Bad Request",
            "",
            "text/plain",
            "no session",
        );
    }
    if exchange.body.contains("notifications/initialized") || exchange.body.is_empty() {
        return respond(&mut stream, "202 Accepted", "", "text/plain", "");
    }
    if exchange.body.contains("tools/list") {
        let body = format!(
            "event: message\ndata: {{\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}}\n\ndata: {{\"jsonrpc\":\"2.0\",\"id\":{id},\ndata: \"result\":{{\"tools\":[{{\"name\":\"search\",\"description\":\"Finds docs\"}}]}}}}\n\n"
        );
        return respond(&mut stream, "200 OK", "", "text/event-stream", &body);
    }
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":{id},"result":{{"content":[{{"type":"text","text":"found it"}}]}}}}"#
    );
    respond(&mut stream, "200 OK", "", "application/json", &body);
}

#[test]
fn an_http_server_keeps_its_session_and_streams_replies() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            serve(stream);
        }
    });
    let url: McpUrl = format!("http://{address}/mcp")
        .parse()
        .unwrap_or_else(|_| unreachable!("a loopback address"));
    let start = McpServerStart {
        server: McpServer {
            name: name("docs"),
            transport: McpTransport::Http {
                url,
                headers: vec![McpVariable {
                    name: variable("Authorization"),
                    value: McpValue::Secret,
                }],
            },
            state: McpServerState::Enabled,
        },
        secrets: HashMap::from([(variable("Authorization"), secret("Bearer token-1"))]),
    };
    let (toolbox, failures) = McpToolbox::start(&[start], Path::new("."));
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(toolbox.tools()[0].name, "mcp__docs__search");
    assert_eq!(
        toolbox
            .call("mcp__docs__search", json!({"q": "mcp"}))
            .ok()
            .as_deref(),
        Some("found it")
    );
    Ok(())
}
