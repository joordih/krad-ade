use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead as _, Write as _};

use crate::client::call;
use crate::code::{CodePosition, CodeRequest, CodeTool};
use crate::env::bus_env;
use crate::handle::Caller;
use crate::protocol::{BusToken, Request, Response};

const SERVER_NAME: &str = "krad-code";
const DEFAULT_PROTOCOL: &str = "2025-06-18";
const TOOL_PREFIX: &str = "code_";
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const OUTSIDE_KRAD: &str = "Code tools only work in agents Krad ADE starts";
const EXIT_OK: i32 = 0;

#[derive(Deserialize)]
struct Incoming {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Deserialize)]
struct CallParams {
    name: String,
    #[serde(default)]
    arguments: CallArguments,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallArguments {
    #[serde(default)]
    path: String,
    #[serde(default)]
    line: Option<u32>,
    #[serde(default)]
    column: Option<u32>,
    #[serde(default)]
    new_name: Option<String>,
}

fn tool_schema(tool: CodeTool) -> Value {
    let mut properties = json!({
        "path": {"type": "string", "description": "File path, absolute or relative to the agent's working folder"},
    });
    let mut required = vec!["path"];
    if tool.needs_position() {
        properties["line"] =
            json!({"type": "integer", "minimum": 1, "description": "One-based line"});
        properties["column"] =
            json!({"type": "integer", "minimum": 1, "description": "One-based column"});
        required.extend(["line", "column"]);
    }
    if tool == CodeTool::Rename {
        properties["newName"] =
            json!({"type": "string", "description": "The new name of the symbol"});
        required.push("newName");
    }
    json!({
        "name": format!("{TOOL_PREFIX}{}", tool.name()),
        "description": tool.description(),
        "inputSchema": {"type": "object", "properties": properties, "required": required},
        "annotations": {"readOnlyHint": !tool.modifies(), "destructiveHint": tool.modifies()},
    })
}

fn request_of(params: CallParams) -> Result<CodeRequest, String> {
    let tool = params
        .name
        .strip_prefix(TOOL_PREFIX)
        .ok_or_else(|| format!("Unknown tool {}", params.name))?
        .parse::<CodeTool>()
        .map_err(|error| error.to_string())?;
    let arguments = params.arguments;
    let position = match (arguments.line, arguments.column) {
        (Some(line), Some(column)) if line > 0 && column > 0 => Some(CodePosition {
            line: line - 1,
            character: column - 1,
        }),
        _ => None,
    };
    if tool.needs_position() && position.is_none() {
        return Err(format!("{tool} needs line and column"));
    }
    if tool == CodeTool::Rename && arguments.new_name.as_deref().is_none_or(str::is_empty) {
        return Err("rename needs newName".to_owned());
    }
    Ok(CodeRequest {
        tool,
        path: arguments.path,
        position,
        new_name: arguments.new_name,
    })
}

fn run_tool(request: CodeRequest) -> (String, bool) {
    let Some(env) = bus_env() else {
        return (OUTSIDE_KRAD.to_owned(), true);
    };
    let Ok(from) = env.terminal.parse::<Caller>() else {
        return (OUTSIDE_KRAD.to_owned(), true);
    };
    match call(
        &env.endpoint,
        &BusToken::from(env.token),
        &from,
        Request::Code(request),
    ) {
        Ok(Response::Code(answer)) => (answer.render(), false),
        Ok(Response::Error(message)) => (message, true),
        Ok(_) => (OUTSIDE_KRAD.to_owned(), true),
        Err(error) => (error.to_string(), true),
    }
}

fn answer(incoming: Incoming) -> Option<Value> {
    let id = incoming.id?;
    let result = match incoming.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": incoming.params.get("protocolVersion").cloned().unwrap_or_else(|| json!(DEFAULT_PROTOCOL)),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": SERVER_NAME, "version": krad_host::HOST_CLI_BUILD},
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": CodeTool::ALL.map(tool_schema)})),
        "tools/call" => match serde_json::from_value::<CallParams>(incoming.params)
            .map_err(|error| error.to_string())
            .and_then(request_of)
        {
            Ok(request) => {
                let (text, failed) = run_tool(request);
                Ok(json!({"content": [{"type": "text", "text": text}], "isError": failed}))
            }
            Err(message) => Err((INVALID_PARAMS, message)),
        },
        other => Err((METHOD_NOT_FOUND, format!("Unknown method {other}"))),
    };
    Some(match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, message)) => {
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
        }
    })
}

pub(crate) fn run_mcp() -> i32 {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(incoming) = serde_json::from_str::<Incoming>(&line) else {
            continue;
        };
        let Some(reply) = answer(incoming) else {
            continue;
        };
        if writeln!(stdout, "{reply}")
            .and_then(|()| stdout.flush())
            .is_err()
        {
            break;
        }
    }
    EXIT_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(method: &str, params: &Value) -> Option<Value> {
        answer(
            serde_json::from_value(
                json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}),
            )
            .ok()?,
        )
    }

    #[test]
    fn every_code_tool_is_listed_with_its_schema() {
        let listed = ask("tools/list", &json!({})).unwrap_or_default();
        let tools = listed["result"]["tools"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(tools.len(), CodeTool::ALL.len());
        let rename = tools
            .iter()
            .find(|tool| tool["name"] == "code_rename")
            .cloned()
            .unwrap_or_default();
        assert_eq!(rename["annotations"]["destructiveHint"], json!(true));
        assert!(rename["inputSchema"]["required"]
            .as_array()
            .is_some_and(|required| required.contains(&json!("newName"))));
    }

    #[test]
    fn a_call_without_a_position_is_refused_before_reaching_the_bus() {
        let refused = ask(
            "tools/call",
            &json!({"name": "code_definition", "arguments": {"path": "Main.java"}}),
        )
        .unwrap_or_default();
        assert_eq!(refused["error"]["code"], json!(INVALID_PARAMS));
    }

    #[test]
    fn notifications_get_no_answer() {
        let notification = serde_json::from_value::<Incoming>(
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        );
        assert!(notification.is_ok_and(|incoming| answer(incoming).is_none()));
    }
}
