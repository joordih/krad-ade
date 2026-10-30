mod http;
mod protocol;
mod stdio;
#[cfg(test)]
mod tests;

use krad_domain::{
    expand_variables, McpSecret, McpServerName, McpServerStart, McpTransport, McpValue,
    McpVariable, McpVariableName,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;
use thiserror::Error;

use http::HttpLink;
use protocol::{
    call_params, call_text, initialize_params, list_params, ListedTool, ToolPage, INITIALIZE,
    INITIALIZED, TOOLS_CALL, TOOLS_LIST,
};
use stdio::{StdioLaunch, StdioLink};

const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(30);
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_LIST_PAGES: usize = 50;
const TOOL_PREFIX: &str = "mcp__";
const TOOL_SEPARATOR: &str = "__";
const MAX_TOOL_NAME: usize = 64;
const NAME_REPLACEMENT: char = '_';
const NAME_PUNCTUATION: [char; 2] = ['_', '-'];
const DUPLICATE_MARK: char = '_';
const MAX_DUPLICATES: usize = 1000;
const STDERR_LEAD: &str = ": ";
const SHOWN_SEPARATOR: &str = " · ";

#[derive(Debug, Error)]
pub enum McpError {
    #[error("the server could not start: {0}")]
    Spawn(std::io::Error),
    #[error("the server stopped{}", stderr_tail(stderr))]
    Exited { stderr: String },
    #[error("the server did not answer within {} seconds", .0.as_secs())]
    TimedOut(Duration),
    #[error("the server could not be reached: {0}")]
    Unreachable(String),
    #[error("the server answered with status {code}{}", stderr_tail(text))]
    Status { code: u16, text: String },
    #[error("the server answered with something that is not MCP")]
    Malformed,
    #[error("the server refused the request: {message}")]
    Server { code: i64, message: String },
    #[error("there is no MCP tool named {0}")]
    UnknownTool(String),
    #[error("{0}")]
    ToolFailed(String),
    #[error("the secret {0} is not saved; add it again in Settings › MCP")]
    MissingSecret(McpVariableName),
}

fn stderr_tail(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        String::new()
    } else {
        format!("{STDERR_LEAD}{text}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpTool {
    pub name: String,
    pub server: McpServerName,
    pub tool: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug)]
pub struct McpStartFailure {
    pub server: McpServerName,
    pub error: McpError,
}

enum Link {
    Stdio(StdioLink),
    Http(HttpLink),
}

struct Connection {
    link: Link,
    next_id: u64,
}

struct Route {
    server: usize,
    tool: String,
}

pub struct McpToolbox {
    connections: Vec<Mutex<Connection>>,
    tools: Vec<McpTool>,
    routes: HashMap<String, Route>,
}

fn from_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn expanded(text: &str) -> String {
    expand_variables(text, from_env).into_owned()
}

fn resolved(
    variables: &[McpVariable],
    secrets: &HashMap<McpVariableName, McpSecret>,
) -> Result<Vec<(String, String)>, McpError> {
    variables
        .iter()
        .map(|variable| {
            let value = match &variable.value {
                McpValue::Plain(text) => expanded(text),
                McpValue::Secret => secrets
                    .get(&variable.name)
                    .map(|secret| secret.expose().to_owned())
                    .ok_or_else(|| McpError::MissingSecret(variable.name.clone()))?,
            };
            Ok((variable.name.as_str().to_owned(), value))
        })
        .collect()
}

impl Connection {
    fn open(start: &McpServerStart, cwd: &Path) -> Result<Self, McpError> {
        let link = match &start.server.transport {
            McpTransport::Stdio { command, args, env } => {
                let args: Vec<String> = args.iter().map(|arg| expanded(arg)).collect();
                Link::Stdio(StdioLink::spawn(&StdioLaunch {
                    command: &expanded(command),
                    args: &args,
                    env: &resolved(env, &start.secrets)?,
                    cwd,
                })?)
            }
            McpTransport::Http { url, headers } => Link::Http(HttpLink::new(
                &expanded(url.as_str()),
                resolved(headers, &start.secrets)?,
            )),
        };
        let mut connection = Self { link, next_id: 0 };
        let reply = connection.request(INITIALIZE, &initialize_params(), INITIALIZE_TIMEOUT)?;
        if !reply.is_object() {
            return Err(McpError::Malformed);
        }
        connection.notify(INITIALIZED)?;
        Ok(connection)
    }

    fn request(
        &mut self,
        method: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<Value, McpError> {
        self.next_id += 1;
        match &mut self.link {
            Link::Stdio(link) => link.request(self.next_id, method, params, timeout),
            Link::Http(link) => link.request(self.next_id, method, params, timeout),
        }
    }

    fn notify(&mut self, method: &str) -> Result<(), McpError> {
        match &mut self.link {
            Link::Stdio(link) => link.notify(method),
            Link::Http(link) => link.notify(method),
        }
    }

    fn list_tools(&mut self) -> Result<Vec<ListedTool>, McpError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_LIST_PAGES {
            let reply = self.request(TOOLS_LIST, &list_params(cursor.as_deref()), LIST_TIMEOUT)?;
            let page: ToolPage = serde_json::from_value(reply).map_err(|_| McpError::Malformed)?;
            tools.extend(page.tools);
            match page.next_cursor.filter(|next| !next.is_empty()) {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        Ok(tools)
    }
}

fn opened(start: &McpServerStart, cwd: &Path) -> Result<(Connection, Vec<ListedTool>), McpError> {
    let mut connection = Connection::open(start, cwd)?;
    let tools = connection.list_tools()?;
    Ok((connection, tools))
}

pub(crate) fn is_mcp_tool(name: &str) -> bool {
    name.starts_with(TOOL_PREFIX)
}

pub(crate) fn shown_tool(name: &str) -> String {
    let Some(rest) = name.strip_prefix(TOOL_PREFIX) else {
        return name.to_owned();
    };
    match rest.split_once(TOOL_SEPARATOR) {
        Some((server, tool)) => format!("{server}{SHOWN_SEPARATOR}{tool}"),
        None => rest.to_owned(),
    }
}

fn clean_part(text: &str) -> String {
    text.chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() || NAME_PUNCTUATION.contains(&char) {
                char
            } else {
                NAME_REPLACEMENT
            }
        })
        .collect()
}

fn truncated(text: &str, limit: usize) -> &str {
    text.get(..limit).unwrap_or(text)
}

fn tool_name(server: &str, tool: &str, taken: &HashSet<String>) -> String {
    let full = format!(
        "{TOOL_PREFIX}{}{TOOL_SEPARATOR}{}",
        clean_part(server),
        clean_part(tool)
    );
    let name = truncated(&full, MAX_TOOL_NAME).to_owned();
    if !taken.contains(&name) {
        return name;
    }
    (2..MAX_DUPLICATES)
        .map(|count| {
            let mark = format!("{DUPLICATE_MARK}{count}");
            let kept = truncated(&full, MAX_TOOL_NAME.saturating_sub(mark.len()));
            format!("{kept}{mark}")
        })
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or(name)
}

fn object_schema(schema: Option<Value>) -> Value {
    schema
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({ "type": "object", "properties": {} }))
}

impl McpToolbox {
    #[must_use]
    pub fn start(starts: &[McpServerStart], cwd: &Path) -> (Self, Vec<McpStartFailure>) {
        let results: Vec<Result<(Connection, Vec<ListedTool>), McpError>> =
            thread::scope(|scope| {
                let handles: Vec<_> = starts
                    .iter()
                    .map(|start| scope.spawn(move || opened(start, cwd)))
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| {
                        handle.join().unwrap_or_else(|_| {
                            Err(McpError::Exited {
                                stderr: String::new(),
                            })
                        })
                    })
                    .collect()
            });
        let mut toolbox = Self {
            connections: Vec::new(),
            tools: Vec::new(),
            routes: HashMap::new(),
        };
        let mut failures = Vec::new();
        let mut taken: HashSet<String> = HashSet::new();
        for (start, result) in starts.iter().zip(results) {
            match result {
                Ok((connection, listed)) => {
                    toolbox.add(&start.server.name, connection, listed, &mut taken);
                }
                Err(error) => failures.push(McpStartFailure {
                    server: start.server.name.clone(),
                    error,
                }),
            }
        }
        (toolbox, failures)
    }

    fn add(
        &mut self,
        server: &McpServerName,
        connection: Connection,
        listed: Vec<ListedTool>,
        taken: &mut HashSet<String>,
    ) {
        let index = self.connections.len();
        self.connections.push(Mutex::new(connection));
        for listed in listed {
            let name = tool_name(server.as_str(), &listed.name, taken);
            taken.insert(name.clone());
            self.routes.insert(
                name.clone(),
                Route {
                    server: index,
                    tool: listed.name.clone(),
                },
            );
            self.tools.push(McpTool {
                name,
                server: server.clone(),
                tool: listed.name,
                description: listed.description.unwrap_or_default(),
                parameters: object_schema(listed.input_schema),
            });
        }
    }

    #[must_use]
    pub fn tools(&self) -> &[McpTool] {
        &self.tools
    }

    /// # Errors
    ///
    /// Fails when no server offers the tool, the server cannot be reached,
    /// does not answer in time, or reports the call as failed.
    pub fn call(&self, name: &str, arguments: Value) -> Result<String, McpError> {
        let route = self
            .routes
            .get(name)
            .ok_or_else(|| McpError::UnknownTool(name.to_owned()))?;
        let connection = self
            .connections
            .get(route.server)
            .ok_or_else(|| McpError::UnknownTool(name.to_owned()))?;
        let arguments = if arguments.is_null() {
            json!({})
        } else {
            arguments
        };
        let reply = connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .request(
                TOOLS_CALL,
                &call_params(&route.tool, arguments),
                CALL_TIMEOUT,
            )?;
        call_text(reply)
    }
}
