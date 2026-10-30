use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use thiserror::Error;

const HTTP_SCHEME: &str = "http://";
const HTTPS_SCHEME: &str = "https://";
const SCHEME_RELATIVE: &str = "//";
const PATH_SEPARATOR: char = '/';
const AUTHORITY_ENDS: [char; 3] = ['/', '?', '#'];
const QUERY_MARK: char = '?';
const FRAGMENT_MARK: char = '#';
const PORT_SEPARATOR: char = ':';
const IPV6_OPEN: char = '[';
const IPV6_CLOSE: char = ']';
const USERINFO_SEPARATOR: char = '@';
const DOMAIN_SEPARATOR: char = '.';
const LOCAL_HOSTS: [&str; 1] = ["localhost"];
const LOCAL_SUFFIXES: [&str; 4] = [".localhost", ".local", ".internal", ".lan"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub(crate) enum UrlError {
    #[error("only http:// and https:// addresses can be fetched")]
    Scheme,
    #[error("the address has no host")]
    NoHost,
    #[error("the address must not carry credentials")]
    Credentials,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WebUrl {
    text: String,
    host: String,
    origin_end: usize,
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

fn strip_scheme(text: &str) -> Option<(&str, &str)> {
    [HTTPS_SCHEME, HTTP_SCHEME].into_iter().find_map(|scheme| {
        let head = text.get(..scheme.len())?;
        head.eq_ignore_ascii_case(scheme)
            .then(|| (head, &text[scheme.len()..]))
    })
}

const fn is_private_v4(address: Ipv4Addr) -> bool {
    address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_unspecified()
        || address.is_broadcast()
}

fn is_private_v6(address: Ipv6Addr) -> bool {
    address.is_loopback()
        || address.is_unspecified()
        || address.is_unique_local()
        || address.is_unicast_link_local()
        || address.to_ipv4_mapped().is_some_and(is_private_v4)
}

impl WebUrl {
    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }

    pub(crate) fn host(&self) -> &str {
        &self.host
    }

    pub(crate) fn is_private(&self) -> bool {
        match self.host.parse::<IpAddr>() {
            Ok(IpAddr::V4(address)) => is_private_v4(address),
            Ok(IpAddr::V6(address)) => is_private_v6(address),
            Err(_) => {
                LOCAL_HOSTS.contains(&self.host.as_str())
                    || LOCAL_SUFFIXES
                        .iter()
                        .any(|suffix| self.host.ends_with(suffix))
                    || !self.host.contains(DOMAIN_SEPARATOR)
            }
        }
    }

    fn scheme(&self) -> &str {
        self.text
            .split_once(SCHEME_RELATIVE)
            .map_or("", |(scheme, _)| scheme)
    }

    pub(crate) fn join(&self, location: &str) -> Result<Self, UrlError> {
        let location = location.trim();
        if strip_scheme(location).is_some() {
            return location.parse();
        }
        if location.starts_with(SCHEME_RELATIVE) {
            return format!("{}{location}", self.scheme()).parse();
        }
        let origin = &self.text[..self.origin_end];
        if location.starts_with(PATH_SEPARATOR) {
            return format!("{origin}{location}").parse();
        }
        let path = &self.text[self.origin_end..];
        let path = path.split(QUERY_MARK).next().unwrap_or_default();
        let folder = path
            .rfind(PATH_SEPARATOR)
            .map_or("", |slash| &path[..slash]);
        format!("{origin}{folder}{PATH_SEPARATOR}{location}").parse()
    }
}

impl FromStr for WebUrl {
    type Err = UrlError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        let text = text
            .split_once(FRAGMENT_MARK)
            .map_or(text, |(kept, _)| kept);
        let (scheme, rest) = strip_scheme(text).ok_or(UrlError::Scheme)?;
        let authority = rest.split(AUTHORITY_ENDS).next().unwrap_or_default();
        if authority.contains(USERINFO_SEPARATOR) {
            return Err(UrlError::Credentials);
        }
        let host = host_of(authority);
        if host.is_empty() || text.chars().any(char::is_whitespace) {
            return Err(UrlError::NoHost);
        }
        Ok(Self {
            text: format!("{}{rest}", scheme.to_ascii_lowercase()),
            host: host.to_ascii_lowercase(),
            origin_end: scheme.len() + authority.len(),
        })
    }
}

impl fmt::Display for WebUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> WebUrl {
        text.parse()
            .unwrap_or_else(|error| unreachable!("{text} parses: {error}"))
    }

    #[test]
    fn only_web_addresses_with_a_host_parse() {
        let page = url("HTTPS://Docs.RS/serde#derive");
        assert_eq!(page.as_str(), "https://Docs.RS/serde");
        assert_eq!(page.host(), "docs.rs");
        assert_eq!("ftp://x.org".parse::<WebUrl>(), Err(UrlError::Scheme));
        assert_eq!("file:///etc/hosts".parse::<WebUrl>(), Err(UrlError::Scheme));
        assert_eq!("https://".parse::<WebUrl>(), Err(UrlError::NoHost));
        assert_eq!(
            "https://me:pw@x.org".parse::<WebUrl>(),
            Err(UrlError::Credentials)
        );
        assert_eq!(url("http://[::1]:8080/a").host(), "::1");
    }

    #[test]
    fn local_and_private_hosts_are_told_apart_from_public_ones() {
        for private in [
            "http://localhost:3000",
            "http://127.0.0.1",
            "http://10.0.0.2",
            "http://192.168.1.1",
            "http://172.16.0.1",
            "http://169.254.169.254/latest",
            "http://[::1]/",
            "http://[fd00::1]/",
            "http://printer.local",
            "http://intranet/",
        ] {
            assert!(url(private).is_private(), "{private}");
        }
        for public in [
            "https://docs.rs",
            "http://93.184.216.34",
            "https://[2606:4700::1]",
        ] {
            assert!(!url(public).is_private(), "{public}");
        }
    }

    #[test]
    fn redirect_locations_resolve_against_the_page() {
        let page = url("https://x.org/docs/guide/intro.html?lang=en");
        let joined = |location: &str| page.join(location).map(|url| url.to_string());
        assert_eq!(joined("/a"), Ok("https://x.org/a".to_owned()));
        assert_eq!(
            joined("next.html"),
            Ok("https://x.org/docs/guide/next.html".to_owned())
        );
        assert_eq!(joined("//cdn.org/b"), Ok("https://cdn.org/b".to_owned()));
        assert_eq!(joined("http://y.org/"), Ok("http://y.org/".to_owned()));
    }
}
