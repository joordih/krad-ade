use super::*;
use crate::engine::provider::tests::{
    conversation, parse, pictured, read_tool, request, GONE_NOTE,
};
use crate::engine::wire::{Call, Delta, Source, Stop, Usage};
use crate::gateway::GatewayError;
use krad_domain::Effort;
use serde_json::json;

const STREAM: &str = r#"event: response.created
data: {"type":"response.created","response":{"id":"resp_1"}}

event: response.reasoning_summary_text.delta
data: {"type":"response.reasoning_summary_text.delta","delta":"Plan"}

event: response.output_item.done
data: {"type":"response.output_item.done","item":{"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"Plan"}],"encrypted_content":"gAAA"}}

event: response.output_text.delta
data: {"type":"response.output_text.delta","delta":"Reading "}

event: response.output_text.delta
data: {"type":"response.output_text.delta","delta":"a.rs"}

event: response.output_item.done
data: {"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Reading a.rs"}]}}

event: response.output_item.done
data: {"type":"response.output_item.done","item":{"type":"function_call","call_id":"call_1","name":"read","arguments":"{\"path\":\"a.rs\"}"}}

event: response.completed
data: {"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":120,"output_tokens":30,"input_tokens_details":{"cached_tokens":100}}}}
"#;

#[test]
fn a_streamed_turn_yields_its_reasoning_text_and_call() {
    let (reply, deltas) = parse(Parser::default(), STREAM);
    assert_eq!(
        deltas,
        [
            Delta::Thinking("Plan".to_owned()),
            Delta::Text("Reading ".to_owned()),
            Delta::Text("a.rs".to_owned()),
        ]
    );
    let Ok(reply) = reply else {
        unreachable!("the stream is well formed: {reply:?}");
    };
    assert_eq!(
        reply.blocks,
        [
            Block::Thinking {
                text: "Plan".to_owned(),
                signature: Some("gAAA".to_owned()),
                protocol: Protocol::Responses,
            },
            Block::Text {
                text: "Reading a.rs".to_owned()
            },
            Block::Call(Call {
                id: "call_1".to_owned(),
                name: "read".to_owned(),
                arguments: json!({"path": "a.rs"}),
            }),
        ]
    );
    assert_eq!(reply.stop, Stop::Tools);
    assert_eq!(
        reply.usage,
        Usage {
            input: 20,
            output: 30,
            cache_read: 100,
            cache_write: 0,
        }
    );
}

#[test]
fn a_cut_answer_stops_for_length_and_a_failure_carries_its_message() {
    let cut = r#"data: {"type":"response.incomplete","response":{"incomplete_details":{"reason":"max_output_tokens"}}}
"#;
    let (reply, _) = parse(Parser::default(), cut);
    assert!(reply.is_ok_and(|reply| reply.stop == Stop::Length));
    let failed = r#"data: {"type":"response.failed","response":{"error":{"message":"usage limit"}}}
"#;
    let (reply, _) = parse(Parser::default(), failed);
    assert!(matches!(reply, Err(GatewayError::Failed(message)) if message == "usage limit"));
}

#[test]
fn the_body_lists_items_with_reasoning_calls_and_outputs() -> serde_json::Result<()> {
    let messages = conversation(Protocol::Responses);
    let tools = read_tool();
    let thinking = Request {
        effort: Some(Effort::Max),
        ..request(&messages, &tools)
    };
    let sent = serde_json::to_value(body(&thinking))?;
    assert_eq!(
        sent,
        json!({
            "model": "test-model",
            "instructions": "be brief",
            "input": [
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "fix it"}]},
                {"type": "reasoning", "encrypted_content": "sig-1", "summary": [{"type": "summary_text", "text": "look first"}]},
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Reading"}]},
                {"type": "function_call", "call_id": "c1", "name": "read", "arguments": "{\"path\":\"a.rs\"}"},
                {"type": "function_call_output", "call_id": "c1", "output": "fn a() {}"},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "and the tests"}]}
            ],
            "tools": [{
                "type": "function",
                "name": "read",
                "description": "Read a file",
                "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
            }],
            "reasoning": {"effort": "high", "summary": "auto"},
            "include": ["reasoning.encrypted_content"],
            "max_output_tokens": 1000,
            "store": false,
            "stream": true
        })
    );
    let foreign = conversation(Protocol::Messages);
    let sent = serde_json::to_value(body(&request(&foreign, &tools)))?;
    assert_eq!(sent["input"][1]["type"], "message");
    Ok(())
}

#[test]
fn native_search_is_a_web_search_tool_and_citations_become_sources() -> serde_json::Result<()> {
    let messages = conversation(Protocol::Responses);
    let mut asked = request(&messages, &[]);
    asked.native = Some(NativeTool::WebSearch);
    let sent = serde_json::to_value(body(&asked))?;
    assert_eq!(sent["tools"], json!([{"type": "web_search"}]));
    let stream = concat!(
        r#"data: {"type":"response.output_item.done","item":{"type":"web_search_call","id":"ws_1","status":"completed"}}"#,
        "\n\n",
        r#"data: {"type":"response.output_item.done","item":{"type":"message","content":[{"type":"output_text","text":"Rust 1.90 is out.","annotations":[{"type":"url_citation","url":"https://blog.rust-lang.org","title":"Rust Blog","start_index":0,"end_index":4}]}]}}"#,
        "\n\n"
    );
    let (reply, _) = parse(Parser::default(), stream);
    let reply = reply.unwrap_or_default();
    assert_eq!(
        reply.sources,
        [Source {
            title: "Rust Blog".to_owned(),
            url: "https://blog.rust-lang.org".to_owned(),
        }]
    );
    Ok(())
}

#[test]
fn user_images_go_as_input_images_with_data_urls() -> serde_json::Result<()> {
    let (messages, attachments) = pictured();
    let asked = Request {
        images: &attachments,
        ..request(&messages, &[])
    };
    let sent = serde_json::to_value(body(&asked))?;
    assert_eq!(
        sent["input"],
        json!([{"type": "message", "role": "user", "content": [
            {"type": "input_image", "image_url": "data:image/png;base64,cG5n", "detail": "auto"},
            {"type": "input_text", "text": GONE_NOTE},
            {"type": "input_text", "text": "what is this?"}
        ]}])
    );
    Ok(())
}
