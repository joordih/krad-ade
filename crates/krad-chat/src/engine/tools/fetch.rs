mod html;
mod url;

use std::io::Read as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use ureq::http::header::{ACCEPT, CONTENT_TYPE, LOCATION, USER_AGENT};
use ureq::http::Response;
use ureq::{Agent, Body};

use super::side::{Asked, SideRequest};
use super::Ran;
use crate::engine::wire::answer_text;
use crate::gateway::GatewayError;
pub(crate) use url::WebUrl;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FETCH_BYTES: u64 = 5 * 1024 * 1024;
const MAX_FETCH_CHARS: usize = 40_000;
const MAX_REDIRECTS: usize = 5;
const NO_REDIRECTS: u32 = 0;
const AGENT_NAME: &str = "Mozilla/5.0 (compatible; KradADE)";
const ACCEPTED: &str = "text/html, text/plain, text/markdown, application/json;q=0.9, */*;q=0.5";
const HTML_MARK: &str = "html";
const TEXT_PREFIX: &str = "text/";
const TEXT_LIKE: [&str; 4] = ["json", "xml", "javascript", "yaml"];
const MARKUP_START: char = '<';
const CANCELLED_TEXT: &str = "[cancelled]";
const TOO_MANY_REDIRECTS_TEXT: &str = "the page redirected too many times";
const NO_LOCATION_TEXT: &str = "the page redirected without saying where";
const NOT_TEXT_TEXT: &str = "the page is not text";
const EMPTY_PAGE_TEXT: &str = "(empty page)";
const CUT_TEXT: &str = "characters cut";
const EXTRACT_MAX_TOKENS: u64 = 8_192;
const EXTRACT_SYSTEM: &str = "You read one web page for a coding agent. Answer the request from the page alone: quote code, names, versions and numbers exactly, keep the links that matter, and say plainly when the page does not hold the answer. Answer with the findings only.";
const EXTRACT_REQUEST_LABEL: &str = "Request:";
const EXTRACT_PAGE_LABEL: &str = "Page";
const EXTRACT_FAILED_TEXT: &str = "the model could not read the page for the prompt";
const EMPTY_EXTRACT_TEXT: &str = "the model answered nothing for the prompt";
const RAW_PAGE_HINT: &str = "Call web_fetch without prompt to read the whole page.";
const NO_MODEL_TEXT: &str = "Pick a Gateway model before fetching a page with a prompt";

enum Arrived {
    Page(WebUrl, Response<Body>),
    Elsewhere(WebUrl),
}

struct Page {
    at: WebUrl,
    text: String,
}

enum Fetched {
    Page(Page),
    Elsewhere(WebUrl),
}

enum Kind {
    Html,
    Text,
    Other(String),
}

fn agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(FETCH_TIMEOUT))
        .max_redirects(NO_REDIRECTS)
        .http_status_as_error(false)
        .build()
        .into()
}

fn follow(agent: &Agent, start: &WebUrl) -> Result<Arrived, String> {
    let mut url = start.clone();
    for _ in 0..=MAX_REDIRECTS {
        let response = agent
            .get(url.as_str())
            .header(USER_AGENT, AGENT_NAME)
            .header(ACCEPT, ACCEPTED)
            .call()
            .map_err(|error| format!("{url}: {error}"))?;
        if !response.status().is_redirection() {
            return Ok(Arrived::Page(url, response));
        }
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| format!("{url}: {NO_LOCATION_TEXT}"))?;
        let next = url
            .join(location)
            .map_err(|error| format!("{url}: {error}"))?;
        if next.host() != start.host() {
            return Ok(Arrived::Elsewhere(next));
        }
        url = next;
    }
    Err(format!("{start}: {TOO_MANY_REDIRECTS_TEXT}"))
}

fn kind_of(content_type: Option<&str>, body: &str) -> Kind {
    let Some(content_type) = content_type.map(str::to_ascii_lowercase) else {
        return if body.trim_start().starts_with(MARKUP_START) {
            Kind::Html
        } else {
            Kind::Text
        };
    };
    if content_type.contains(HTML_MARK) {
        Kind::Html
    } else if content_type.starts_with(TEXT_PREFIX)
        || TEXT_LIKE.iter().any(|like| content_type.contains(like))
    {
        Kind::Text
    } else {
        Kind::Other(content_type)
    }
}

fn clip(text: &str) -> String {
    let count = text.chars().count();
    if count <= MAX_FETCH_CHARS {
        return text.to_owned();
    }
    let head: String = text.chars().take(MAX_FETCH_CHARS).collect();
    format!("{head}\n… [{} {CUT_TEXT}]", count - MAX_FETCH_CHARS)
}

fn read_page(url: WebUrl, response: Response<Body>) -> Result<Page, String> {
    let code = response.status().as_u16();
    let failed = !response.status().is_success();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_FETCH_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{url}: {error}"))?;
    let body = String::from_utf8_lossy(&bytes);
    let text = match kind_of(content_type.as_deref(), &body) {
        Kind::Html => html::readable(&body, &url),
        Kind::Text => body.trim().to_owned(),
        Kind::Other(kind) => return Err(format!("{url}: {NOT_TEXT_TEXT} ({kind})")),
    };
    let text = if text.is_empty() {
        EMPTY_PAGE_TEXT.to_owned()
    } else {
        clip(&text)
    };
    if failed {
        return Err(format!("{url} answered with status {code}\n\n{text}"));
    }
    Ok(Page { at: url, text })
}

fn load(url: &WebUrl, cancel: &AtomicBool) -> Result<Fetched, String> {
    let arrived = follow(&agent(), url);
    if cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED_TEXT.to_owned());
    }
    match arrived? {
        Arrived::Page(at, response) => read_page(at, response).map(Fetched::Page),
        Arrived::Elsewhere(next) => Ok(Fetched::Elsewhere(next)),
    }
}

fn elsewhere(url: &WebUrl, next: &WebUrl) -> Ran {
    Ran::done(format!(
        "{url} redirected to {next}, on another host. Call web_fetch with that address to read it."
    ))
}

pub(super) fn fetch(url: &WebUrl, cancel: &AtomicBool) -> Ran {
    match load(url, cancel) {
        Ok(Fetched::Page(page)) => Ran::done(format!("{}\n\n{}", page.at, page.text)),
        Ok(Fetched::Elsewhere(next)) => elsewhere(url, &next),
        Err(error) => Ran::failed(error),
    }
}

fn extract(page: &Page, prompt: &str, side: &SideRequest, cancel: &AtomicBool) -> Ran {
    let asked = Asked {
        system: EXTRACT_SYSTEM,
        text: format!(
            "{EXTRACT_REQUEST_LABEL} {prompt}\n\n{EXTRACT_PAGE_LABEL} {}:\n\n{}",
            page.at, page.text
        ),
        native: None,
        max_tokens: EXTRACT_MAX_TOKENS,
    };
    match side.ask(asked, cancel) {
        Ok(reply) => {
            let found = answer_text(&reply.blocks);
            let found = found.trim();
            if found.is_empty() {
                return Ran::failed(format!(
                    "{}: {EMPTY_EXTRACT_TEXT}. {RAW_PAGE_HINT}",
                    page.at
                ));
            }
            Ran::done(format!("{}\n\n{found}", page.at))
        }
        Err(GatewayError::Cancelled) => Ran::failed(CANCELLED_TEXT.to_owned()),
        Err(error) => Ran::failed(format!(
            "{}: {EXTRACT_FAILED_TEXT} ({error}). {RAW_PAGE_HINT}",
            page.at
        )),
    }
}

pub(super) fn fetch_for(
    url: &WebUrl,
    prompt: &str,
    side: Option<&SideRequest>,
    cancel: &AtomicBool,
) -> Ran {
    let Some(side) = side else {
        return Ran::failed(NO_MODEL_TEXT.to_owned());
    };
    match load(url, cancel) {
        Ok(Fetched::Page(page)) => extract(&page, prompt, side, cancel),
        Ok(Fetched::Elsewhere(next)) => elsewhere(url, &next),
        Err(error) => Ran::failed(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    fn serve(answers: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|_| unreachable!());
        let address = listener.local_addr().unwrap_or_else(|_| unreachable!());
        thread::spawn(move || {
            for (mut stream, answer) in listener.incoming().flatten().zip(answers) {
                let mut reader =
                    BufReader::new(stream.try_clone().unwrap_or_else(|_| unreachable!()));
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|read| read > 2) {
                    line.clear();
                }
                stream.write_all(answer.as_bytes()).unwrap_or_default();
            }
        });
        format!("http://{address}")
    }

    fn page(kind: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn url(text: &str) -> WebUrl {
        text.parse()
            .unwrap_or_else(|_| unreachable!("{text} parses"))
    }

    #[test]
    fn a_page_comes_back_as_readable_text_after_same_host_redirects() {
        let base = serve(vec![
            "HTTP/1.1 301 Moved\r\nLocation: /docs\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_owned(),
            page("text/html; charset=utf-8", "<h1>Docs</h1><p>Hello &amp; welcome</p>"),
        ]);
        let ran = fetch(&url(&format!("{base}/old")), &AtomicBool::new(false));
        assert!(!ran.failed, "{}", ran.content);
        assert_eq!(
            ran.content,
            format!("{base}/docs\n\n# Docs\nHello & welcome")
        );
    }

    #[test]
    fn a_redirect_to_another_host_is_reported_not_followed() {
        let base = serve(vec![
            "HTTP/1.1 302 Found\r\nLocation: https://elsewhere.org/x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_owned(),
        ]);
        let ran = fetch(&url(&base), &AtomicBool::new(false));
        assert!(!ran.failed);
        assert!(ran
            .content
            .contains("redirected to https://elsewhere.org/x"));
    }

    #[test]
    fn binary_pages_and_errors_fail_and_long_text_is_cut() {
        let base = serve(vec![
            page("image/png", "PNG"),
            "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: 4\r\nConnection: close\r\n\r\ngone"
                .to_owned(),
        ]);
        let binary = fetch(&url(&base), &AtomicBool::new(false));
        assert!(binary.failed && binary.content.contains("image/png"));
        let missing = fetch(&url(&base), &AtomicBool::new(false));
        assert!(missing.failed && missing.content.contains("status 404"));
        let long = "x".repeat(MAX_FETCH_CHARS + 10);
        assert!(clip(&long).ends_with("[10 characters cut]"));
    }
}
