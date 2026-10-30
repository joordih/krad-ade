use krad_chat::{new_session_id, AskAnswer, AskKind, LiveEvent, LiveSession};
use krad_domain::{McpServer, McpServerStart, McpServerState, McpTransport};
use std::collections::HashMap;
use std::fs;
use std::io;

use super::support::{
    call_stream, completions, config, fake_gateway, finished, scratch, start, until, ANSWER,
};

const FAKE_SERVER: &str = r#"
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\),.*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}}}}\n' "$id" ;;
    *'"method":"tools/list"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"greet","description":"Says hello","inputSchema":{"type":"object","properties":{"who":{"type":"string"}}}}]}}\n' "$id" ;;
    *'"method":"tools/call"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"hello from mcp"}]}}\n' "$id" ;;
  esac
done
"#;

fn server(name: &str, script: &str) -> McpServerStart {
    McpServerStart {
        server: McpServer {
            name: name
                .parse()
                .unwrap_or_else(|_| unreachable!("{name} is a server name")),
            transport: McpTransport::Stdio {
                command: "sh".to_owned(),
                args: vec!["-c".to_owned(), script.to_owned()],
                env: Vec::new(),
            },
            state: McpServerState::Enabled,
        },
        secrets: HashMap::new(),
    }
}

#[test]
fn an_mcp_tool_asks_once_when_allowed_always_and_its_answer_goes_back() -> io::Result<()> {
    let root = scratch("mcp");
    fs::create_dir_all(root.join("work"))?;
    let greet = serde_json::json!({"who": "krad"});
    let (url, seen) = fake_gateway(vec![
        call_stream("call_m1", "mcp__fake__greet", &greet),
        call_stream("call_m2", "mcp__fake__greet", &greet),
        ANSWER.to_owned(),
    ])?;
    let mut chat_config = config(&url, &root);
    chat_config.mcp = vec![server("fake", FAKE_SERVER)];
    let (chat, events) = start(chat_config, &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("greet me");
    let asked = until(&events, |event| matches!(event, LiveEvent::Ask(_)));
    let Some(LiveEvent::Ask(ask)) = asked.last() else {
        unreachable!("an MCP tool asks in Manual mode: {asked:?}");
    };
    let AskKind::Permission { tool, summary, .. } = &ask.kind else {
        unreachable!("an MCP call asks for permission: {:?}", ask.kind);
    };
    assert_eq!(
        (tool.as_str(), summary.as_str()),
        ("fake · greet", "who: krad")
    );
    chat.answer(ask, &AskAnswer::AllowAlways);
    let answered = until(&events, finished);
    assert!(
        !answered
            .iter()
            .any(|event| matches!(event, LiveEvent::Ask(_))),
        "the second call of the same tool is remembered"
    );
    drop(chat);
    let bodies = completions(&seen);
    assert_eq!(bodies.len(), 3);
    assert!(bodies[0].contains("mcp__fake__greet") && bodies[0].contains("Says hello"));
    assert!(bodies[1].contains("hello from mcp"), "{}", bodies[1]);
    Ok(())
}

#[test]
fn a_server_that_does_not_start_is_told_to_the_chat() -> io::Result<()> {
    let root = scratch("mcp-fail");
    fs::create_dir_all(root.join("work"))?;
    let (url, _) = fake_gateway(Vec::new())?;
    let mut chat_config = config(&url, &root);
    chat_config.mcp = vec![server("broken", "echo boom >&2; exit 1")];
    let (_chat, events) = start(chat_config, &LiveSession::Start(new_session_id()));
    let opened = until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    assert!(opened.iter().any(|event| matches!(
        event,
        LiveEvent::Notice(notice) if notice.contains("broken") && notice.contains("boom")
    )));
    Ok(())
}
