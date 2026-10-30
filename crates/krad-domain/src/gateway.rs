use crate::http_url::{checked_http_url, HttpUrlError, PATH_SEPARATOR};
use crate::model::{lenient_models, ModelId};
use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

pub const EXAMPLE_GATEWAY_URL: &str = "http://127.0.0.1:8317";
const REDACTED: &str = "[REDACTED]";
const API_VERSION_SUFFIX: &str = "/v1";
const DEFAULT_CONTEXT_TOKENS: u64 = 200_000;

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

impl FromStr for GatewayUrl {
    type Err = HttpUrlError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let base = checked_http_url(text)?.trim_end_matches(PATH_SEPARATOR);
        let base = base.strip_suffix(API_VERSION_SUFFIX).unwrap_or(base);
        Ok(Self(base.trim_end_matches(PATH_SEPARATOR).to_owned()))
    }
}

impl TryFrom<String> for GatewayUrl {
    type Error = HttpUrlError;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct ContextWindow(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmptyContextWindow;

impl fmt::Display for EmptyContextWindow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a context window holds at least one token")
    }
}

impl std::error::Error for EmptyContextWindow {}

impl ContextWindow {
    pub const DEFAULT: Self = Self(DEFAULT_CONTEXT_TOKENS);

    #[must_use]
    pub const fn tokens(self) -> u64 {
        self.0
    }
}

impl Default for ContextWindow {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl TryFrom<u64> for ContextWindow {
    type Error = EmptyContextWindow;

    fn try_from(tokens: u64) -> Result<Self, Self::Error> {
        if tokens == 0 {
            return Err(EmptyContextWindow);
        }
        Ok(Self(tokens))
    }
}

impl From<ContextWindow> for u64 {
    fn from(window: ContextWindow) -> Self {
        window.0
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredWindow {
    Valid(ContextWindow),
    Invalid(IgnoredAny),
}

fn lenient_windows<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<HashMap<ModelId, ContextWindow>, D::Error> {
    let stored = HashMap::<String, StoredWindow>::deserialize(deserializer)?;
    Ok(stored
        .into_iter()
        .filter_map(|(model, window)| match window {
            StoredWindow::Valid(window) => Some((ModelId::parse(&model)?, window)),
            StoredWindow::Invalid(_) => None,
        })
        .collect())
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewaySettings {
    #[serde(
        default,
        deserialize_with = "valid_url",
        skip_serializing_if = "Option::is_none"
    )]
    pub url: Option<GatewayUrl>,
    #[serde(
        default,
        deserialize_with = "lenient_models",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub models: Vec<ModelId>,
    #[serde(
        default,
        deserialize_with = "lenient_windows",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub context_windows: HashMap<ModelId, ContextWindow>,
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

    fn url(text: &str) -> Result<String, HttpUrlError> {
        text.parse::<GatewayUrl>().map(String::from)
    }

    #[test]
    fn the_api_version_and_trailing_slashes_are_dropped() {
        assert_eq!(
            url(" http://localhost:8317/ "),
            Ok("http://localhost:8317".to_owned())
        );
        assert_eq!(
            url(&format!("{EXAMPLE_GATEWAY_URL}/v1/")),
            Ok(EXAMPLE_GATEWAY_URL.to_owned())
        );
        assert_eq!(
            url("https://example.test/gateway/v1"),
            Ok("https://example.test/gateway".to_owned())
        );
        assert_eq!(
            url("http://10.0.0.2:8317/v1"),
            Err(HttpUrlError::PlainHttpToRemote)
        );
    }

    #[test]
    fn a_stored_address_is_checked_again_when_read() -> serde_json::Result<()> {
        let read: GatewaySettings = serde_json::from_str(r#"{"url":"http://10.0.0.2:8317"}"#)?;
        assert_eq!(read.url, None);
        let stored = format!(r#"{{"url":"{EXAMPLE_GATEWAY_URL}"}}"#);
        let kept: GatewaySettings = serde_json::from_str(&stored)?;
        assert_eq!(
            kept.url.map(String::from),
            Some(EXAMPLE_GATEWAY_URL.to_owned())
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
    fn context_windows_keep_valid_overrides_and_drop_the_rest() -> serde_json::Result<()> {
        let read: GatewaySettings = serde_json::from_str(
            r#"{"context_windows":{"gemini-3-pro":1000000,"bad id":5,"gpt-5":0,"o3":"big"}}"#,
        )?;
        let gemini = ModelId::parse("gemini-3-pro").unwrap_or_else(|| unreachable!("valid"));
        assert_eq!(
            read.context_windows.into_iter().collect::<Vec<_>>(),
            [(
                gemini,
                ContextWindow::try_from(1_000_000).unwrap_or_default()
            )]
        );
        assert_eq!(ContextWindow::default().tokens(), DEFAULT_CONTEXT_TOKENS);
        assert!(!serde_json::to_string(&GatewaySettings::default())?.contains("context_windows"));
        Ok(())
    }

    #[test]
    fn toggling_a_model_adds_then_removes_it() {
        let mut settings = GatewaySettings::default();
        let model = ModelId::parse("gemini-3-pro").unwrap_or_else(|| unreachable!("valid"));
        settings.toggle_model(&model);
        assert_eq!(settings.models, std::slice::from_ref(&model));
        settings.toggle_model(&model);
        assert_eq!(settings.models, Vec::<ModelId>::new());
    }
}
