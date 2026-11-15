use std::io::{self, Read, Write};

const HEAD_END: &[u8] = b"\r\n\r\n";
const MAX_HEAD_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 4 * 1024;
const READ_CHUNK: usize = 4096;
const SECURITY_HEADERS: &str = "Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\n\
X-Content-Type-Options: nosniff\r\n\
Referrer-Policy: no-referrer\r\n\
Cross-Origin-Opener-Policy: same-origin\r\n\
Cross-Origin-Resource-Policy: same-origin\r\n";
pub(crate) const STRICT_TRANSPORT: &str = "Strict-Transport-Security: max-age=31536000\r\n";

#[derive(Debug, thiserror::Error)]
pub(crate) enum HttpError {
    #[error("the connection failed")]
    Io(#[from] io::Error),
    #[error("the request head is too large or malformed")]
    BadHead,
    #[error("the request body is too large")]
    BodyTooLarge,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Request {
    pub(crate) method: String,
    pub(crate) path: String,
    headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

impl Request {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn has_token(&self, name: &str, token: &str) -> bool {
        self.header(name).is_some_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case(token))
        })
    }

    pub(crate) fn closes(&self) -> bool {
        self.has_token("connection", "close")
    }
}

fn head_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(HEAD_END.len())
        .position(|window| window == HEAD_END)
}

fn parse_head(head: &str) -> Option<Request> {
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next()?.split(' ');
    let method = request_line.next()?.to_owned();
    let path = request_line.next()?.to_owned();
    request_line
        .next()
        .filter(|version| version.starts_with("HTTP/1."))?;
    let headers = lines
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_owned(), value.trim().to_owned()))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Request {
        method,
        path,
        headers,
        body: Vec::new(),
    })
}

pub(crate) fn read_request(
    input: &mut impl Read,
    pending: &mut Vec<u8>,
) -> Result<Option<Request>, HttpError> {
    let mut chunk = [0; READ_CHUNK];
    let end = loop {
        if let Some(end) = head_end(pending) {
            break end;
        }
        if pending.len() > MAX_HEAD_BYTES {
            return Err(HttpError::BadHead);
        }
        let read = input.read(&mut chunk)?;
        if read == 0 {
            return if pending.is_empty() {
                Ok(None)
            } else {
                Err(HttpError::BadHead)
            };
        }
        pending.extend_from_slice(&chunk[..read]);
    };
    let head = std::str::from_utf8(&pending[..end]).map_err(|_| HttpError::BadHead)?;
    let mut request = parse_head(head).ok_or(HttpError::BadHead)?;
    pending.drain(..end + HEAD_END.len());
    let length = match request.header("content-length") {
        Some(raw) => raw.parse::<usize>().map_err(|_| HttpError::BadHead)?,
        None => 0,
    };
    if length > MAX_BODY_BYTES {
        return Err(HttpError::BodyTooLarge);
    }
    while pending.len() < length {
        let read = input.read(&mut chunk)?;
        if read == 0 {
            return Err(HttpError::BadHead);
        }
        pending.extend_from_slice(&chunk[..read]);
    }
    request.body = pending.drain(..length).collect();
    Ok(Some(request))
}

pub(crate) struct Response<'a> {
    pub(crate) status: u16,
    pub(crate) reason: &'a str,
    pub(crate) content_type: &'a str,
    pub(crate) body: &'a [u8],
    pub(crate) extra: Vec<String>,
}

impl<'a> Response<'a> {
    pub(crate) const fn new(status: u16, reason: &'a str) -> Self {
        Self {
            status,
            reason,
            content_type: "text/plain; charset=utf-8",
            body: &[],
            extra: Vec::new(),
        }
    }

    pub(crate) fn with_body(mut self, content_type: &'a str, body: &'a [u8]) -> Self {
        self.content_type = content_type;
        self.body = body;
        self
    }

    pub(crate) fn with_header(mut self, header: String) -> Self {
        self.extra.push(header);
        self
    }

    pub(crate) fn write(&self, output: &mut impl Write, secure: bool) -> io::Result<()> {
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\n{SECURITY_HEADERS}",
            self.status,
            self.reason,
            self.content_type,
            self.body.len()
        );
        if secure {
            head.push_str(STRICT_TRANSPORT);
        }
        for header in &self.extra {
            head.push_str(header);
            head.push_str("\r\n");
        }
        head.push_str("\r\n");
        output.write_all(head.as_bytes())?;
        output.write_all(self.body)?;
        output.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_reads_its_head_and_body_and_leaves_the_next_one(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let raw = b"POST /pairing HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\n\r\n{}GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let mut input = &raw[..];
        let mut pending = Vec::new();
        let first = read_request(&mut input, &mut pending)?.ok_or("no first request")?;
        assert_eq!(first.method, "POST");
        assert_eq!(first.body, b"{}");
        assert_eq!(first.header("HOST"), Some("localhost"));
        let second = read_request(&mut input, &mut pending)?.ok_or("no second request")?;
        assert_eq!(second.path, "/");
        assert!(read_request(&mut input, &mut pending)?.is_none());
        Ok(())
    }

    #[test]
    fn an_oversized_body_is_refused_before_it_is_read() {
        let raw = b"POST /pairing HTTP/1.1\r\nContent-Length: 999999\r\n\r\n";
        let mut pending = Vec::new();
        assert!(matches!(
            read_request(&mut &raw[..], &mut pending),
            Err(HttpError::BodyTooLarge)
        ));
    }
}
