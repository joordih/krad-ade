use crate::agent::AgentPreset;
use crate::model::{clean_models, ModelId};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::str::FromStr;

const REDACTED: &str = "[REDACTED]";
const HTTPS_SCHEME: &str = "https://";
const HTTP_SCHEME: &str = "http://";
const API_VERSION_SUFFIX: &str = "/v1";
const PATH_SEPARATOR: char = '/';
const PORT_SEPARATOR: char = ':';
const IPV6_OPEN: char = '[';
const IPV6_CLOSE: char = ']';
const USERINFO_SEPARATOR: char = '@';
const LOCALHOST: &str = "localhost";
const IPV4_LOOPBACK_PREFIX: &str = "127.";
const IPV6_LOOPBACK: &str = "::1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    #[default]
    Direct,
    Gateway,
}

impl Route {
    pub const ALL: [Self; 2] = [Self::Direct, Self::Gateway];

    #[must_use]
    pub const fn is_direct(&self) -> bool {
        matches!(self, Self::Direct)
    }

    #[must_use]
    pub const fn reachable_by(preset: AgentPreset) -> bool {
        matches!(preset, AgentPreset::ClaudeCode)
    }

    #[must_use]
    pub const fn for_harness(self, preset: AgentPreset) -> Self {
        if Self::reachable_by(preset) {
            self
        } else {
            Self::Direct
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Direct => "Direct",
            Self::Gateway => "Gateway",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GatewayUrlError {
    Empty,
    NoScheme,
    NoHost,
    PlainHttpToRemote,
    Credentials,
}

impl fmt::Display for GatewayUrlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "Enter the Gateway address",
            Self::NoScheme => "The address starts with http:// or https://",
            Self::NoHost => "The address has no host",
            Self::PlainHttpToRemote => {
                "Use https:// for a Gateway on another machine; plain http:// is only for this one"
            }
            Self::Credentials => "Put the key in its own field, not in the address",
        })
    }
}

impl std::error::Error for GatewayUrlError {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct GatewayUrl(String);

impl GatewayUrl {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn join(&self, path: &str) -> String {
        format!("{}{path}", self.0)
    }
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

impl FromStr for GatewayUrl {
    type Err = GatewayUrlError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.is_empty() {
            return Err(GatewayUrlError::Empty);
        }
        let lowered = text.to_ascii_lowercase();
        let (secure, rest) = if lowered.starts_with(HTTPS_SCHEME) {
            (true, &text[HTTPS_SCHEME.len()..])
        } else if lowered.starts_with(HTTP_SCHEME) {
            (false, &text[HTTP_SCHEME.len()..])
        } else {
            return Err(GatewayUrlError::NoScheme);
        };
        if rest.chars().any(char::is_whitespace) {
            return Err(GatewayUrlError::NoHost);
        }
        let authority = rest
            .split_once(PATH_SEPARATOR)
            .map_or(rest, |(authority, _)| authority);
        if authority.contains(USERINFO_SEPARATOR) {
            return Err(GatewayUrlError::Credentials);
        }
        let host = host_of(authority);
        if host.is_empty() {
            return Err(GatewayUrlError::NoHost);
        }
        if !secure && !is_loopback(host) {
            return Err(GatewayUrlError::PlainHttpToRemote);
        }
        let base = text.trim_end_matches(PATH_SEPARATOR);
        let base = base.strip_suffix(API_VERSION_SUFFIX).unwrap_or(base);
        Ok(Self(base.trim_end_matches(PATH_SEPARATOR).to_owned()))
    }
}

impl TryFrom<String> for GatewayUrl {
    type Error = GatewayUrlError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<GatewayUrl> for String {
    fn from(url: GatewayUrl) -> Self {
        url.0
    }
}

impl fmt::Display for GatewayUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GatewayKey(String);

impl GatewayKey {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        (!text.is_empty()).then(|| Self(text.to_owned()))
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for GatewayKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayAccess {
    pub url: GatewayUrl,
    pub key: GatewayKey,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewaySettings {
    #[serde(
        default,
        deserialize_with = "valid_url",
        skip_serializing_if = "Option::is_none"
    )]
    pub url: Option<GatewayUrl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<ModelId>,
    #[serde(default, skip_serializing_if = "Route::is_direct")]
    pub default_route: Route,
}

fn valid_url<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<GatewayUrl>, D::Error> {
    let raw = Option::<String>::deserialize(deserializer)?;
    Ok(raw.and_then(|raw| raw.parse().ok()))
}

impl GatewaySettings {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    #[must_use]
    pub fn models(&self) -> Vec<ModelId> {
        clean_models(&self.models)
    }

    #[must_use]
    pub fn new_chat_route(&self, preset: AgentPreset) -> Route {
        if self.url.is_none() {
            return Route::Direct;
        }
        self.default_route.for_harness(preset)
    }

    pub fn toggle_model(&mut self, model: &ModelId) {
        match self.models.iter().position(|kept| kept == model) {
            Some(at) => {
                self.models.remove(at);
            }
            None => self.models.push(model.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Result<String, GatewayUrlError> {
        text.parse::<GatewayUrl>().map(String::from)
    }

    #[test]
    fn plain_http_is_only_for_this_machine() {
        assert_eq!(
            url("http://127.0.0.1:8317"),
            Ok("http://127.0.0.1:8317".to_owned())
        );
        assert_eq!(
            url(" http://localhost:8317/ "),
            Ok("http://localhost:8317".to_owned())
        );
        assert_eq!(url("http://[::1]:8317"), Ok("http://[::1]:8317".to_owned()));
        assert_eq!(
            url("http://192.168.1.20:8317"),
            Err(GatewayUrlError::PlainHttpToRemote)
        );
        assert_eq!(
            url("https://proxy.example.test"),
            Ok("https://proxy.example.test".to_owned())
        );
    }

    #[test]
    fn the_api_version_and_trailing_slashes_are_dropped() {
        assert_eq!(
            url("http://127.0.0.1:8317/v1/"),
            Ok("http://127.0.0.1:8317".to_owned())
        );
        assert_eq!(
            url("https://example.test/gateway/v1"),
            Ok("https://example.test/gateway".to_owned())
        );
    }

    #[test]
    fn addresses_without_scheme_host_or_with_credentials_are_refused() {
        assert_eq!(url(""), Err(GatewayUrlError::Empty));
        assert_eq!(url("127.0.0.1:8317"), Err(GatewayUrlError::NoScheme));
        assert_eq!(url("http://"), Err(GatewayUrlError::NoHost));
        assert_eq!(url("http://:8317"), Err(GatewayUrlError::NoHost));
        assert_eq!(
            url("https://user:secret@example.test"),
            Err(GatewayUrlError::Credentials)
        );
    }

    #[test]
    fn a_stored_address_is_checked_again_when_read() -> serde_json::Result<()> {
        let read: GatewaySettings = serde_json::from_str(r#"{"url":"http://10.0.0.2:8317"}"#)?;
        assert_eq!(read.url, None);
        let kept: GatewaySettings = serde_json::from_str(r#"{"url":"http://127.0.0.1:8317"}"#)?;
        assert_eq!(
            kept.url.map(String::from),
            Some("http://127.0.0.1:8317".to_owned())
        );
        Ok(())
    }

    #[test]
    fn the_key_never_prints() {
        let key = GatewayKey::parse(" sk-secret ").unwrap_or_else(|| unreachable!("not blank"));
        assert_eq!(key.expose(), "sk-secret");
        assert!(!format!("{key:?}").contains("secret"));
        assert_eq!(GatewayKey::parse("  "), None);
    }

    #[test]
    fn only_claude_code_takes_the_gateway_route_and_only_once_set_up() {
        let mut settings = GatewaySettings {
            default_route: Route::Gateway,
            ..GatewaySettings::default()
        };
        assert_eq!(
            settings.new_chat_route(AgentPreset::ClaudeCode),
            Route::Direct
        );
        settings.url = "http://127.0.0.1:8317".parse().ok();
        assert_eq!(
            settings.new_chat_route(AgentPreset::ClaudeCode),
            Route::Gateway
        );
        assert_eq!(settings.new_chat_route(AgentPreset::Codex), Route::Direct);
    }

    #[test]
    fn toggling_a_model_adds_then_removes_it() {
        let mut settings = GatewaySettings::default();
        let model = ModelId::parse("gemini-3-pro").unwrap_or_else(|| unreachable!("valid"));
        settings.toggle_model(&model);
        assert_eq!(settings.models(), std::slice::from_ref(&model));
        settings.toggle_model(&model);
        assert_eq!(settings.models(), Vec::<ModelId>::new());
    }
}
