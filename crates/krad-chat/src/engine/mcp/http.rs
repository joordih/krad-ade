use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::time::Duration;
use ureq::{Agent, Body};

use super::protocol::{notification_line, request_line, Incoming, Received, PROTOCOL_VERSION};
use super::McpError;

const CONTENT_TYPE: &str = "Content-Type";
const ACCEPT: &str = "Accept";
const JSON_TYPE: &str = "application/json";
const EVENT_STREAM_TYPE: &str = "text/event-stream";
const ACCEPT_BOTH: &str = "application/json, text/event-stream";
const SESSION_HEADER: &str = "Mcp-Session-Id";
const VERSION_HEADER: &str = "MCP-Protocol-Version";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const ERROR_BODY_LIMIT: u64 = 4096;
const DATA_FIELD: &str = "data:";
const DATA_JOINER: char = '\n';
const VALUE_LEAD: char = ' ';
const CARRIAGE_RETURN: char = '\r';

pub(super) struct HttpLink {
    agent: Agent,
    url: String,
    headers: Vec<(String, String)>,
    session: Option<String>,
    initialized: bool,
}

fn data_events(body: Body) -> impl Iterator<Item = String> {
    let mut lines = BufReader::new(body.into_reader()).lines();
    std::iter::from_fn(move || {
        let mut data: Option<String> = None;
        loop {
            let Some(Ok(line)) = lines.next() else {
                return data;
            };
            let line = line.trim_end_matches(CARRIAGE_RETURN);
            if line.is_empty() {
                if data.is_some() {
                    return data;
                }
                continue;
            }
            let Some(value) = line.strip_prefix(DATA_FIELD) else {
                continue;
            };
            let value = value.strip_prefix(VALUE_LEAD).unwrap_or(value);
            match data.as_mut() {
                Some(joined) => {
                    joined.push(DATA_JOINER);
                    joined.push_str(value);
                }
                None => data = Some(value.to_owned()),
            }
        }
    })
}

fn reply_in(messages: impl Iterator<Item = Incoming>, id: u64) -> Result<Value, McpError> {
    for message in messages {
        if let Received::Reply(reply) = message.sort(id) {
            return reply;
        }
    }
    Err(McpError::Malformed)
}

impl HttpLink {
    pub(super) fn new(url: &str, headers: Vec<(String, String)>) -> Self {
        let agent: Agent = Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            agent,
            url: url.to_owned(),
            headers,
            session: None,
            initialized: false,
        }
    }

    fn post(
        &self,
        line: String,
        timeout: Duration,
    ) -> Result<ureq::http::Response<Body>, McpError> {
        let mut request = self
            .agent
            .post(&self.url)
            .header(CONTENT_TYPE, JSON_TYPE)
            .header(ACCEPT, ACCEPT_BOTH);
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }
        if let Some(session) = &self.session {
            request = request.header(SESSION_HEADER, session);
        }
        if self.initialized {
            request = request.header(VERSION_HEADER, PROTOCOL_VERSION);
        }
        let mut response = request
            .config()
            .timeout_global(Some(timeout))
            .build()
            .send(line)
            .map_err(|error| match error {
                ureq::Error::Timeout(_) => McpError::TimedOut(timeout),
                error => McpError::Unreachable(error.to_string()),
            })?;
        if !response.status().is_success() {
            let text = response
                .body_mut()
                .with_config()
                .limit(ERROR_BODY_LIMIT)
                .read_to_string()
                .unwrap_or_default();
            return Err(McpError::Status {
                code: response.status().as_u16(),
                text,
            });
        }
        Ok(response)
    }

    pub(super) fn notify(&mut self, method: &str) -> Result<(), McpError> {
        self.initialized = true;
        self.post(notification_line(method), CONNECT_TIMEOUT)
            .map(drop)
    }

    pub(super) fn request(
        &mut self,
        id: u64,
        method: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<Value, McpError> {
        let response = self.post(request_line(id, method, params), timeout)?;
        if let Some(session) = response
            .headers()
            .get(SESSION_HEADER)
            .and_then(|value| value.to_str().ok())
        {
            self.session = Some(session.to_owned());
        }
        let streamed = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|kind| kind.starts_with(EVENT_STREAM_TYPE));
        let body = response.into_body();
        if streamed {
            let messages =
                data_events(body).filter_map(|data| serde_json::from_str::<Incoming>(&data).ok());
            return reply_in(messages, id);
        }
        let mut body = body;
        let text = body
            .read_to_string()
            .map_err(|error| McpError::Unreachable(error.to_string()))?;
        let message: Incoming = serde_json::from_str(&text).map_err(|_| McpError::Malformed)?;
        reply_in(std::iter::once(message), id)
    }
}

impl Drop for HttpLink {
    fn drop(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        let mut request = self.agent.delete(&self.url).header(SESSION_HEADER, session);
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }
        let closed = request
            .config()
            .timeout_global(Some(CLOSE_TIMEOUT))
            .build()
            .call();
        drop(closed);
    }
}
