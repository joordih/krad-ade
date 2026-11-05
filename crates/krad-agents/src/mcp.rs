use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead as _, Write as _};

use crate::client::call;
use crate::code::{CodePosition, CodeRequest, CodeTool};
use crate::env::bus_env;
use crate::extension_tools::{is_extension_tool, mcp_schema, ExtensionToolOutcome, ToolCatalog};
use crate::handle::Caller;
use crate::protocol::{BusToken, Request, Response};
use krad_domain::CodeTools;

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
    arguments: Value,
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

pub(crate) trait Bus {
    fn ask(&self, request: Request) -> Result<Response, String>;
}

struct EnvBus;

impl Bus for EnvBus {
    fn ask(&self, request: Request) -> Result<Response, String> {
        let env = bus_env().ok_or_else(|| OUTSIDE_KRAD.to_owned())?;
        let from = env
            .terminal
            .parse::<Caller>()
            .map_err(|_| OUTSIDE_KRAD.to_owned())?;
        call(&env.endpoint, &BusToken::from(env.token), &from, request)
            .map_err(|error| error.to_string())
    }
}

fn code_request(name: &str, arguments: Value) -> Result<CodeRequest, String> {
    let tool = name
        .strip_prefix(TOOL_PREFIX)
        .ok_or_else(|| format!("Unknown tool {name}"))?
        .parse::<CodeTool>()
        .map_err(|error| error.to_string())?;
    let arguments: CallArguments = if arguments.is_null() {
        CallArguments::default()
    } else {
        serde_json::from_value(arguments).map_err(|error| error.to_string())?
    };
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

fn run_code(bus: &impl Bus, request: CodeRequest) -> (String, bool) {
    match bus.ask(Request::Code(request)) {
        Ok(Response::Code(answer)) => (answer.render(), false),
        Ok(Response::Error(message)) | Err(message) => (message, true),
        Ok(_) => (OUTSIDE_KRAD.to_owned(), true),
    }
}

fn run_extension_tool(bus: &impl Bus, name: String, arguments: Value) -> (String, bool) {
    match bus.ask(Request::ExtensionTool { name, arguments }) {
        Ok(Response::ExtensionTool(answer)) => {
            (answer.text, answer.outcome == ExtensionToolOutcome::Failed)
        }
        Ok(Response::Error(message)) | Err(message) => (message, true),
        Ok(_) => (OUTSIDE_KRAD.to_owned(), true),
    }
}

fn listed_tools(bus: &impl Bus) -> Vec<Value> {
    let catalog = match bus.ask(Request::Tools) {
        Ok(Response::Tools(catalog)) => catalog,
        _ => ToolCatalog::default(),
    };
    let code = (catalog.code_tools == CodeTools::On)
        .then(|| CodeTool::ALL.map(tool_schema))
        .into_iter()
        .flatten();
    code.chain(catalog.extension_tools.iter().map(mcp_schema))
        .collect()
}

fn call_tool(bus: &impl Bus, params: Value) -> Result<Value, (i64, String)> {
    let params = serde_json::from_value::<CallParams>(params)
        .map_err(|error| (INVALID_PARAMS, error.to_string()))?;
    let (text, failed) = if is_extension_tool(&params.name) {
        run_extension_tool(bus, params.name, params.arguments)
    } else {
        let request = code_request(&params.name, params.arguments)
            .map_err(|message| (INVALID_PARAMS, message))?;
        run_code(bus, request)
    };
    Ok(json!({"content": [{"type": "text", "text": text}], "isError": failed}))
}

fn answer(incoming: Incoming, bus: &impl Bus) -> Option<Value> {
    let id = incoming.id?;
    let result = match incoming.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": incoming.params.get("protocolVersion").cloned().unwrap_or_else(|| json!(DEFAULT_PROTOCOL)),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": SERVER_NAME, "version": krad_host::HOST_CLI_BUILD},
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": listed_tools(bus)})),
        "tools/call" => call_tool(bus, incoming.params),
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
        let Some(reply) = answer(incoming, &EnvBus) else {
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
mod tests;
