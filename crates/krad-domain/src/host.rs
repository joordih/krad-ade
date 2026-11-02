use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const USER_SEPARATOR: char = '@';
const PORT_SEPARATOR: char = ':';
const BRACKET_OPEN: char = '[';
const BRACKET_CLOSE: char = ']';
const OPTION_PREFIX: char = '-';
const LOCAL_LABEL: &str = "This computer";
const HOST_KEYWORD: &str = "Host";
const COMMENT_MARK: char = '#';
const OPTION_SEPARATOR: char = '=';
const PATTERN_MARKS: [char; 3] = ['*', '?', '!'];

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Host {
    #[default]
    Local,
    Ssh {
        destination: SshDestination,
    },
}

impl Host {
    #[must_use]
    pub const fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    #[must_use]
    pub const fn destination(&self) -> Option<&SshDestination> {
        match self {
            Self::Local => None,
            Self::Ssh { destination } => Some(destination),
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => f.write_str(LOCAL_LABEL),
            Self::Ssh { destination } => destination.fmt(f),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SshDestination {
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SshDestinationError {
    Empty,
    LooksLikeAnOption,
    InvalidCharacter,
    NoHost,
    InvalidPort,
}

impl fmt::Display for SshDestinationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "Enter the SSH destination",
            Self::LooksLikeAnOption => "The SSH destination cannot start with '-'",
            Self::InvalidCharacter => "The SSH destination cannot contain spaces",
            Self::NoHost => "The SSH destination has no host",
            Self::InvalidPort => "The SSH port is a number from 1 to 65535",
        })
    }
}

impl std::error::Error for SshDestinationError {}

impl SshDestination {
    #[must_use]
    pub fn target(&self) -> String {
        match &self.user {
            Some(user) => format!("{user}{USER_SEPARATOR}{}", self.host),
            None => self.host.clone(),
        }
    }
}

impl FromStr for SshDestination {
    type Err = SshDestinationError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(SshDestinationError::Empty);
        }
        if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(SshDestinationError::InvalidCharacter);
        }
        let (user, rest) = match raw.rsplit_once(USER_SEPARATOR) {
            Some((user, rest)) => (Some(user), rest),
            None => (None, raw),
        };
        let (host, port) = split_port(rest)?;
        if host.is_empty() || user.is_some_and(str::is_empty) {
            return Err(SshDestinationError::NoHost);
        }
        if raw.starts_with(OPTION_PREFIX) || host.starts_with(OPTION_PREFIX) {
            return Err(SshDestinationError::LooksLikeAnOption);
        }
        Ok(Self {
            user: user.map(str::to_owned),
            host: host.to_owned(),
            port,
        })
    }
}

fn split_port(rest: &str) -> Result<(&str, Option<u16>), SshDestinationError> {
    if let Some(inner) = rest.strip_prefix(BRACKET_OPEN) {
        let (host, after) = inner
            .split_once(BRACKET_CLOSE)
            .ok_or(SshDestinationError::NoHost)?;
        return match after.strip_prefix(PORT_SEPARATOR) {
            Some(port) => Ok((host, Some(parse_port(port)?))),
            None if after.is_empty() => Ok((host, None)),
            None => Err(SshDestinationError::InvalidPort),
        };
    }
    match rest.split_once(PORT_SEPARATOR) {
        Some((host, port)) => Ok((host, Some(parse_port(port)?))),
        None => Ok((rest, None)),
    }
}

fn parse_port(raw: &str) -> Result<u16, SshDestinationError> {
    raw.parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or(SshDestinationError::InvalidPort)
}

impl TryFrom<String> for SshDestination {
    type Error = SshDestinationError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        raw.parse()
    }
}

impl From<SshDestination> for String {
    fn from(destination: SshDestination) -> Self {
        destination.to_string()
    }
}

impl fmt::Display for SshDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(user) = &self.user {
            write!(f, "{user}{USER_SEPARATOR}")?;
        }
        let bracketed = self.host.contains(PORT_SEPARATOR);
        if bracketed {
            write!(f, "{BRACKET_OPEN}{}{BRACKET_CLOSE}", self.host)?;
        } else {
            f.write_str(&self.host)?;
        }
        if let Some(port) = self.port {
            write!(f, "{PORT_SEPARATOR}{port}")?;
        }
        Ok(())
    }
}

const SSH_DIR: &str = ".ssh";
const SSH_CONFIG_FILE: &str = "config";

#[must_use]
pub fn ssh_dir(home: &std::path::Path) -> std::path::PathBuf {
    home.join(SSH_DIR)
}

#[must_use]
pub fn ssh_config_file(home: &std::path::Path) -> std::path::PathBuf {
    ssh_dir(home).join(SSH_CONFIG_FILE)
}

#[must_use]
pub fn ssh_config_aliases(config: &str) -> Vec<String> {
    let mut aliases: Vec<String> = Vec::new();
    for line in config.lines().map(str::trim) {
        if line.starts_with(COMMENT_MARK) {
            continue;
        }
        let Some((keyword, value)) =
            line.split_once(|c: char| c.is_whitespace() || c == OPTION_SEPARATOR)
        else {
            continue;
        };
        if !keyword.eq_ignore_ascii_case(HOST_KEYWORD) {
            continue;
        }
        let value = value.trim_start_matches(OPTION_SEPARATOR);
        for alias in value.split_whitespace() {
            let concrete = !alias.contains(PATTERN_MARKS);
            if concrete
                && alias.parse::<SshDestination>().is_ok()
                && !aliases.iter().any(|seen| seen == alias)
            {
                aliases.push(alias.to_owned());
            }
        }
    }
    aliases
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn an_alias_is_a_destination_on_its_own() -> TestResult {
        let destination: SshDestination = "devbox".parse()?;
        assert_eq!(destination.user, None);
        assert_eq!(destination.host, "devbox");
        assert_eq!(destination.port, None);
        assert_eq!(destination.target(), "devbox");
        Ok(())
    }

    #[test]
    fn user_host_and_port_are_split() -> TestResult {
        let destination: SshDestination = "jordi@10.0.0.5:2222".parse()?;
        assert_eq!(destination.user.as_deref(), Some("jordi"));
        assert_eq!(destination.host, "10.0.0.5");
        assert_eq!(destination.port, Some(2222));
        assert_eq!(destination.target(), "jordi@10.0.0.5");
        assert_eq!(destination.to_string(), "jordi@10.0.0.5:2222");
        Ok(())
    }

    #[test]
    fn ipv6_hosts_take_their_port_after_brackets() -> TestResult {
        let destination: SshDestination = "root@[::1]:22".parse()?;
        assert_eq!(destination.host, "::1");
        assert_eq!(destination.port, Some(22));
        assert_eq!(destination.to_string(), "root@[::1]:22");
        Ok(())
    }

    #[test]
    fn ipv6_hosts_without_a_port_keep_their_brackets() -> TestResult {
        for raw in ["[::1]", "user@[fe80::1]", "root@[::1]:22"] {
            let destination: SshDestination = raw.parse()?;
            assert_eq!(destination.to_string(), raw);
            assert_eq!(
                destination.to_string().parse::<SshDestination>()?,
                destination
            );
        }
        Ok(())
    }

    #[test]
    fn destinations_that_ssh_would_read_as_options_are_refused() {
        assert_eq!(
            "-oProxyCommand=evil".parse::<SshDestination>(),
            Err(SshDestinationError::LooksLikeAnOption)
        );
        assert_eq!(
            "user@-oProxyCommand=evil".parse::<SshDestination>(),
            Err(SshDestinationError::LooksLikeAnOption)
        );
    }

    #[test]
    fn malformed_destinations_are_refused() {
        assert_eq!(
            "".parse::<SshDestination>(),
            Err(SshDestinationError::Empty)
        );
        assert_eq!(
            "dev box".parse::<SshDestination>(),
            Err(SshDestinationError::InvalidCharacter)
        );
        assert_eq!(
            "@host".parse::<SshDestination>(),
            Err(SshDestinationError::NoHost)
        );
        assert_eq!(
            "host:0".parse::<SshDestination>(),
            Err(SshDestinationError::InvalidPort)
        );
        assert_eq!(
            "host:ssh".parse::<SshDestination>(),
            Err(SshDestinationError::InvalidPort)
        );
    }

    #[test]
    fn hosts_roundtrip_through_json() -> TestResult {
        let host = Host::Ssh {
            destination: "jordi@devbox:2200".parse()?,
        };
        let json = serde_json::to_string(&host)?;
        assert_eq!(json, r#"{"kind":"ssh","destination":"jordi@devbox:2200"}"#);
        assert_eq!(serde_json::from_str::<Host>(&json)?, host);
        assert!(serde_json::from_str::<Host>(r#"{"kind":"ssh","destination":"-x"}"#).is_err());
        Ok(())
    }

    #[test]
    fn config_aliases_skip_patterns_comments_and_repeats() {
        let config = "# personal\nHost devbox build-01\n  HostName 10.0.0.5\nHost *\n  ServerAliveInterval 30\nhost=gpu\nHost !bastion devbox prod-?\nMatch host x\n";
        assert_eq!(ssh_config_aliases(config), ["devbox", "build-01", "gpu"]);
    }
}
