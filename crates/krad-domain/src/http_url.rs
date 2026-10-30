use std::fmt;

const HTTPS_SCHEME: &str = "https://";
const HTTP_SCHEME: &str = "http://";
pub(crate) const PATH_SEPARATOR: char = '/';
const PORT_SEPARATOR: char = ':';
const IPV6_OPEN: char = '[';
const IPV6_CLOSE: char = ']';
const USERINFO_SEPARATOR: char = '@';
const LOCALHOST: &str = "localhost";
const IPV4_LOOPBACK_PREFIX: &str = "127.";
const IPV6_LOOPBACK: &str = "::1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpUrlError {
    Empty,
    NoScheme,
    NoHost,
    PlainHttpToRemote,
    Credentials,
}

impl fmt::Display for HttpUrlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "Enter the address",
            Self::NoScheme => "The address starts with http:// or https://",
            Self::NoHost => "The address has no host",
            Self::PlainHttpToRemote => {
                "Use https:// for a server on another machine; plain http:// is only for this one"
            }
            Self::Credentials => {
                "Keep credentials out of the address: put them in the key or a header"
            }
        })
    }
}

impl std::error::Error for HttpUrlError {}

pub(crate) fn checked_http_url(text: &str) -> Result<&str, HttpUrlError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(HttpUrlError::Empty);
    }
    let lowered = text.to_ascii_lowercase();
    let (secure, rest) = if lowered.starts_with(HTTPS_SCHEME) {
        (true, &text[HTTPS_SCHEME.len()..])
    } else if lowered.starts_with(HTTP_SCHEME) {
        (false, &text[HTTP_SCHEME.len()..])
    } else {
        return Err(HttpUrlError::NoScheme);
    };
    if rest.chars().any(char::is_whitespace) {
        return Err(HttpUrlError::NoHost);
    }
    let authority = rest
        .split_once(PATH_SEPARATOR)
        .map_or(rest, |(authority, _)| authority);
    if authority.contains(USERINFO_SEPARATOR) {
        return Err(HttpUrlError::Credentials);
    }
    let host = host_of(authority);
    if host.is_empty() {
        return Err(HttpUrlError::NoHost);
    }
    if !secure && !is_loopback(host) {
        return Err(HttpUrlError::PlainHttpToRemote);
    }
    Ok(text)
}

fn host_of(authority: &str) -> &str {
    if let Some(bracketed) = authority.strip_prefix(IPV6_OPEN) {
        return bracketed
            .split_once(IPV6_CLOSE)
            .map_or(bracketed, |(host, _)| host);
    }
    authority
        .split_once(PORT_SEPARATOR)
        .map_or(authority, |(host, _)| host)
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case(LOCALHOST)
        || host.starts_with(IPV4_LOOPBACK_PREFIX)
        || host == IPV6_LOOPBACK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_keep_their_path_and_plain_http_stays_on_this_machine() {
        assert_eq!(
            checked_http_url(" https://mcp.example.test/mcp/ "),
            Ok("https://mcp.example.test/mcp/")
        );
        for local in [
            "http://127.0.0.1:3000/mcp",
            "HTTP://localhost:8317/",
            "http://[::1]:8317",
        ] {
            assert_eq!(checked_http_url(local), Ok(local), "{local}");
        }
        assert_eq!(
            checked_http_url("http://192.168.1.20:8317"),
            Err(HttpUrlError::PlainHttpToRemote)
        );
    }

    #[test]
    fn addresses_without_scheme_host_or_with_credentials_are_refused() {
        assert_eq!(checked_http_url("  "), Err(HttpUrlError::Empty));
        assert_eq!(
            checked_http_url("127.0.0.1:8317"),
            Err(HttpUrlError::NoScheme)
        );
        assert_eq!(checked_http_url("http://"), Err(HttpUrlError::NoHost));
        assert_eq!(checked_http_url("http://:8317"), Err(HttpUrlError::NoHost));
        assert_eq!(
            checked_http_url("https://example .test"),
            Err(HttpUrlError::NoHost)
        );
        assert_eq!(
            checked_http_url("https://user:secret@example.test"),
            Err(HttpUrlError::Credentials)
        );
    }
}
