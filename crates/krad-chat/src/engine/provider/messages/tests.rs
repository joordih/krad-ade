use super::*;
use crate::engine::provider::tests::{
    conversation, parse, pictured, read_tool, request, GONE_NOTE,
};
use crate::engine::provider::ProviderError;
use crate::engine::wire::{Call, Delta, Stop, Usage};
use serde_json::json;

const STREAM: &str = r#"event: message_start
data: {"type":"message_start","message":{"usage":{"input_tokens":12,"cache_read_input_tokens":100,"cache_creation_input_tokens":5,"output_tokens":1}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Check the file"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"EqQB"}}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Let me "}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"read it."}}

event: content_block_start
data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"read","input":{}}}

event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}

event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"\"a.rs\"}"}}

event: ping
data: {"type":"ping"}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":40}}

event: message_stop
data: {"type":"message_stop"}
"#;

#[test]
fn a_streamed_turn_yields_thinking_text_and_a_call_in_order() {
    let (reply, deltas) = parse(Parser::default(), STREAM);
    assert_eq!(
        deltas,
        [
            Delta::Thinking("Check the file".to_owned()),
            Delta::Text("Let me ".to_owned()),
            Delta::Text("read it.".to_owned()),
        ]
    );
    let Ok(reply) = reply else {
        unreachable!("the stream is well formed: {reply:?}");
    };
    assert_eq!(
        reply.blocks,
        [
            Block::Thinking {
                text: "Check the file".to_owned(),
                signature: Some("EqQB".to_owned()),
                protocol: Protocol::Messages,
            },
            Block::Text {
                text: "Let me read it.".to_owned()
            },
            Block::Call(Call {
                id: "toolu_1".to_owned(),
                name: "read".to_owned(),
                arguments: json!({"path": "a.rs"}),
            }),
        ]
    );
    assert_eq!(reply.stop, Stop::Tools);
    assert_eq!(
        reply.usage,
        Usage {
            input: 12,
            output: 40,
            cache_read: 100,
            cache_write: 5,
        }
    );
}

#[test]
fn broken_arguments_become_an_empty_object_and_errors_stop_the_turn() {
    let broken = r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t","name":"read","input":{}}}

data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"pa"}}

data: {"type":"message_delta","delta":{"stop_reason":"max_tokens"}}
"#;
    let (reply, _) = parse(Parser::default(), broken);
    assert!(reply.is_ok_and(|reply| reply.stop == Stop::Length
        && reply.blocks
            == [Block::Call(Call {
                id: "t".to_owned(),
                name: "read".to_owned(),
                arguments: json!({}),
            })]));
    let failed = r#"data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}
"#;
    let (reply, _) = parse(Parser::default(), failed);
    assert!(matches!(reply, Err(ProviderError::Failed(message)) if message == "Overloaded"));
}

#[test]
fn the_body_carries_tools_results_and_only_its_own_thinking() -> serde_json::Result<()> {
    let messages = conversation(Protocol::Messages);
    let tools = read_tool();
    let sent = serde_json::to_value(body(&request(&messages, &tools)))?;
    assert_eq!(
        sent,
        json!({
            "model": "test-model",
            "max_tokens": 1000,
            "system": "be brief",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "fix it"}]},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "look first", "signature": "sig-1"},
                    {"type": "text", "text": "Reading"},
                    {"type": "tool_use", "id": "c1", "name": "read", "input": {"path": "a.rs"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "c1", "content": "fn a() {}"},
                    {"type": "text", "text": "and the tests"}
                ]}
            ],
            "tools": [{
                "name": "read",
                "description": "Read a file",
                "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
            }],
            "stream": true
        })
    );
    let foreign = conversation(Protocol::Responses);
    let sent = serde_json::to_value(body(&request(&foreign, &tools)))?;
    assert_eq!(sent["messages"][1]["content"][0]["type"], "text");
    Ok(())
}

#[test]
fn a_summary_after_compaction_and_empty_turns_keep_roles_alternating() -> serde_json::Result<()> {
    let messages = [
        Message::user("summary of earlier work".to_owned()),
        Message::Assistant {
            blocks: vec![Block::Thinking {
                text: "foreign".to_owned(),
                signature: None,
                protocol: Protocol::Completions,
            }],
            usage: Usage::default(),
            stop: Stop::Finished,
        },
        Message::user("go on".to_owned()),
    ];
    let sent = serde_json::to_value(body(&request(&messages, &[])))?;
    assert_eq!(
        sent["messages"],
        json!([{"role": "user", "content": [
            {"type": "text", "text": "summary of earlier work"},
            {"type": "text", "text": "go on"}
        ]}])
    );
    Ok(())
}

#[test]
fn effort_turns_thinking_on_with_room_left_for_the_answer() -> serde_json::Result<()> {
    let messages = [Message::user("hi".to_owned())];
    let thinking = Request {
        effort: Some(Effort::High),
        ..request(&messages, &[])
    };
    let sent = serde_json::to_value(body(&thinking))?;
    assert_eq!(
        sent["thinking"],
        json!({"type": "enabled", "budget_tokens": HIGH_BUDGET})
    );
    assert_eq!(sent["max_tokens"], json!(HIGH_BUDGET + ANSWER_ROOM));
    Ok(())
}

const SEARCH_STREAM: &str = r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"server_tool_use","id":"srvtoolu_1","name":"web_search","input":{"query":"rust"}}}

data: {"type":"content_block_start","index":1,"content_block":{"type":"web_search_tool_result","tool_use_id":"srvtoolu_1","content":[{"type":"web_search_result","url":"https://blog.rust-lang.org","title":"Rust Blog","encrypted_content":"x"}]}}

data: {"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}}

data: {"type":"content_block_delta","index":2,"delta":{"type":"citations_delta","citation":{"type":"web_search_result_location","url":"https://doc.rust-lang.org","title":"Docs","cited_text":"..."}}}

data: {"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"Rust 1.90 is out."}}

data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":9}}
"#;

#[test]
fn native_search_goes_as_a_server_tool_and_its_sources_come_back() -> serde_json::Result<()> {
    let messages = conversation(Protocol::Messages);
    let mut asked = request(&messages, &[]);
    asked.native = Some(NativeTool::WebSearch);
    let sent = serde_json::to_value(body(&asked))?;
    assert_eq!(
        sent["tools"],
        json!([{"type": "web_search_20250305", "name": "web_search", "max_uses": 5}])
    );
    let (reply, _) = parse(Parser::default(), SEARCH_STREAM);
    let reply = reply.unwrap_or_default();
    let urls: Vec<&str> = reply
        .sources
        .iter()
        .map(|source| source.url.as_str())
        .collect();
    assert_eq!(
        urls,
        ["https://blog.rust-lang.org", "https://doc.rust-lang.org"]
    );
    assert_eq!(
        reply.blocks,
        [Block::Text {
            text: "Rust 1.90 is out.".to_owned()
        }]
    );
    Ok(())
}

#[test]
fn user_images_go_as_base64_blocks_before_the_text() -> serde_json::Result<()> {
    let (messages, attachments) = pictured();
    let asked = Request {
        images: &attachments,
        ..request(&messages, &[])
    };
    let sent = serde_json::to_value(body(&asked))?;
    assert_eq!(
        sent["messages"],
        json!([{"role": "user", "content": [
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "cG5n"}},
            {"type": "text", "text": GONE_NOTE},
            {"type": "text", "text": "what is this?"}
        ]}])
    );
    Ok(())
}
