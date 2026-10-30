use super::*;
use crate::engine::provider::tests::{
    conversation, parse, pictured, read_tool, request, GONE_NOTE,
};
use crate::engine::provider::ProviderError;
use crate::engine::wire::{Call, Delta, Protocol, Stop, Usage};
use serde_json::json;

const STREAM: &str = r#"data: {"choices":[{"index":0,"delta":{"role":"assistant","reasoning_content":"Think"}}]}

data: {"choices":[{"index":0,"delta":{"content":"Reading "}}]}

data: {"choices":[{"index":0,"delta":{"content":"a.rs"}}]}

data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_9","type":"function","function":{"name":"read","arguments":""}}]}}]}

data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":"}}]}}]}

data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"a.rs\"}"}}]}}]}

data: {"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}

data: {"choices":[],"usage":{"prompt_tokens":50,"completion_tokens":9,"prompt_tokens_details":{"cached_tokens":10}}}

data: [DONE]
"#;

#[test]
fn a_streamed_turn_folds_reasoning_text_and_indexed_calls() {
    let (reply, deltas) = parse(Parser::default(), STREAM);
    assert_eq!(
        deltas,
        [
            Delta::Thinking("Think".to_owned()),
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
                text: "Think".to_owned(),
                signature: None,
                protocol: Protocol::Completions,
            },
            Block::Text {
                text: "Reading a.rs".to_owned()
            },
            Block::Call(Call {
                id: "call_9".to_owned(),
                name: "read".to_owned(),
                arguments: json!({"path": "a.rs"}),
            }),
        ]
    );
    assert_eq!(reply.stop, Stop::Tools);
    assert_eq!(
        reply.usage,
        Usage {
            input: 40,
            output: 9,
            cache_read: 10,
            cache_write: 0,
        }
    );
}

#[test]
fn a_cut_answer_stops_for_length_and_an_error_chunk_fails() {
    let cut = r#"data: {"choices":[{"delta":{"content":"par"},"finish_reason":"length"}]}
"#;
    let (reply, _) = parse(Parser::default(), cut);
    assert!(reply.is_ok_and(|reply| reply.stop == Stop::Length));
    let failed = r#"data: {"error":{"message":"rate limited"}}
"#;
    let (reply, _) = parse(Parser::default(), failed);
    assert!(matches!(reply, Err(ProviderError::Failed(message)) if message == "rate limited"));
}

#[test]
fn the_body_sends_the_system_first_and_one_tool_message_per_result() -> serde_json::Result<()> {
    let messages = conversation(Protocol::Completions);
    let tools = read_tool();
    let thinking = Request {
        effort: Some(Effort::Low),
        ..request(&messages, &tools)
    };
    let sent = serde_json::to_value(body(&thinking))?;
    assert_eq!(
        sent,
        json!({
            "model": "test-model",
            "messages": [
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": "fix it"},
                {"role": "assistant", "content": "Reading", "tool_calls": [{
                    "id": "c1",
                    "type": "function",
                    "function": {"name": "read", "arguments": "{\"path\":\"a.rs\"}"}
                }]},
                {"role": "tool", "tool_call_id": "c1", "content": "fn a() {}"},
                {"role": "user", "content": "and the tests"}
            ],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "read",
                    "description": "Read a file",
                    "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
                }
            }],
            "reasoning_effort": "low",
            "max_tokens": 1000,
            "stream": true,
            "stream_options": {"include_usage": true}
        })
    );
    Ok(())
}

#[test]
fn native_search_asks_for_google_search_next_to_no_functions() -> serde_json::Result<()> {
    let messages = conversation(Protocol::Completions);
    let mut asked = request(&messages, &[]);
    asked.native = Some(NativeTool::WebSearch);
    let sent = serde_json::to_value(body(&asked))?;
    assert_eq!(sent["tools"], json!([{"google_search": {}}]));
    let tools = read_tool();
    let plain = serde_json::to_value(body(&request(&messages, &tools)))?;
    assert_eq!(plain["tools"][0]["type"], "function");
    Ok(())
}

#[test]
fn user_images_turn_the_content_into_parts_with_image_urls() -> serde_json::Result<()> {
    let (messages, attachments) = pictured();
    let asked = Request {
        images: &attachments,
        ..request(&messages, &[])
    };
    let sent = serde_json::to_value(body(&asked))?;
    assert_eq!(
        sent["messages"][1],
        json!({"role": "user", "content": [
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,cG5n", "detail": "auto"}},
            {"type": "text", "text": GONE_NOTE},
            {"type": "text", "text": "what is this?"}
        ]})
    );
    let plain = [Message::user("hi".to_owned())];
    let sent = serde_json::to_value(body(&request(&plain, &[])))?;
    assert_eq!(
        sent["messages"][1],
        json!({"role": "user", "content": "hi"})
    );
    Ok(())
}
