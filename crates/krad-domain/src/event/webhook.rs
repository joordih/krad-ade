use crate::event::EventKind;
use crate::http_url::{checked_http_url, HttpUrlError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

const REDACTED: &str = "[REDACTED]";
const WEBHOOK_NAME_MAX_CHARS: usize = 64;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct WebhookId(pub u64);

impl fmt::Display for WebhookId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WebhookUrl(String);

impl WebhookUrl {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for WebhookUrl {
    type Err = HttpUrlError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        checked_http_url(text).map(|url| Self(url.to_owned()))
    }
}

impl TryFrom<String> for WebhookUrl {
    type Error = HttpUrlError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<WebhookUrl> for String {
    fn from(url: WebhookUrl) -> Self {
        url.0
    }
}

impl fmt::Display for WebhookUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebhookNameError {
    Empty,
    TooLong,
}

impl fmt::Display for WebhookNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("Enter a name for the webhook"),
            Self::TooLong => write!(
                formatter,
                "Keep the name to {WEBHOOK_NAME_MAX_CHARS} characters"
            ),
        }
    }
}

impl std::error::Error for WebhookNameError {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WebhookName(String);

impl WebhookName {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for WebhookName {
    type Err = WebhookNameError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.is_empty() {
            return Err(WebhookNameError::Empty);
        }
        if text.chars().count() > WEBHOOK_NAME_MAX_CHARS {
            return Err(WebhookNameError::TooLong);
        }
        Ok(Self(text.to_owned()))
    }
}

impl TryFrom<String> for WebhookName {
    type Error = WebhookNameError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<WebhookName> for String {
    fn from(name: WebhookName) -> Self {
        name.0
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WebhookSecret(String);

impl WebhookSecret {
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

impl fmt::Debug for WebhookSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookSigning {
    #[default]
    Unsigned,
    Signed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Webhook {
    pub id: WebhookId,
    pub name: WebhookName,
    pub url: WebhookUrl,
    #[serde(default)]
    pub kinds: BTreeSet<EventKind>,
    #[serde(default)]
    pub signing: WebhookSigning,
}

impl Webhook {
    #[must_use]
    pub fn wants(&self, kind: EventKind) -> bool {
        kind == EventKind::Ping || self.kinds.contains(&kind)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookSettings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<Webhook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_id: Option<WebhookId>,
}

impl WebhookSettings {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty() && self.last_id.is_none()
    }

    pub fn issue_id(&mut self) -> WebhookId {
        let highest = self
            .hooks
            .iter()
            .map(|hook| hook.id)
            .chain(self.last_id)
            .max()
            .unwrap_or_default();
        let issued = WebhookId(highest.0.saturating_add(1));
        self.last_id = Some(issued);
        issued
    }

    pub fn save(&mut self, hook: Webhook) {
        self.last_id = self.last_id.max(Some(hook.id));
        match self.hooks.iter_mut().find(|kept| kept.id == hook.id) {
            Some(kept) => *kept = hook,
            None => self.hooks.push(hook),
        }
    }

    pub fn remove(&mut self, id: WebhookId) -> Option<Webhook> {
        let index = self.hooks.iter().position(|hook| hook.id == id)?;
        Some(self.hooks.remove(index))
    }

    #[must_use]
    pub fn find(&self, id: WebhookId) -> Option<&Webhook> {
        self.hooks.iter().find(|hook| hook.id == id)
    }

    pub fn wanting(&self, kind: EventKind) -> impl Iterator<Item = &Webhook> {
        self.hooks
            .iter()
            .filter(move |hook| kind != EventKind::Ping && hook.wants(kind))
    }

    #[must_use]
    pub fn wants_code_host(&self) -> bool {
        self.hooks
            .iter()
            .any(|hook| hook.kinds.iter().any(|kind| kind.comes_from_code_host()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(id: u64, kinds: &[EventKind]) -> Webhook {
        Webhook {
            id: WebhookId(id),
            name: "ci"
                .parse()
                .unwrap_or_else(|_| unreachable!("a valid name")),
            url: "https://hooks.example.test/krad"
                .parse()
                .unwrap_or_else(|_| unreachable!("a valid url")),
            kinds: kinds.iter().copied().collect(),
            signing: WebhookSigning::Signed,
        }
    }

    #[test]
    fn a_webhook_url_must_be_safe_to_post_to() {
        assert!("https://hooks.example.test/x".parse::<WebhookUrl>().is_ok());
        assert!("http://127.0.0.1:9000/x".parse::<WebhookUrl>().is_ok());
        assert_eq!(
            "http://hooks.example.test/x".parse::<WebhookUrl>(),
            Err(HttpUrlError::PlainHttpToRemote)
        );
        assert_eq!(
            "https://user:pw@hooks.example.test".parse::<WebhookUrl>(),
            Err(HttpUrlError::Credentials)
        );
    }

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(
            " ci ".parse::<WebhookName>().map(|name| name.0),
            Ok("ci".to_owned())
        );
        assert_eq!("  ".parse::<WebhookName>(), Err(WebhookNameError::Empty));
        let long = "x".repeat(WEBHOOK_NAME_MAX_CHARS + 1);
        assert_eq!(long.parse::<WebhookName>(), Err(WebhookNameError::TooLong));
    }

    #[test]
    fn a_secret_never_prints() {
        let secret = WebhookSecret::parse(" s3cr3t ");
        assert_eq!(secret.as_ref().map(WebhookSecret::expose), Some("s3cr3t"));
        assert_eq!(format!("{secret:?}"), format!("Some({REDACTED})"));
        assert_eq!(WebhookSecret::parse("  "), None);
    }

    #[test]
    fn hooks_are_saved_replaced_and_removed_by_id() {
        let mut settings = WebhookSettings::default();
        settings.save(hook(1, &[EventKind::AgentWaiting]));
        settings.save(hook(2, &[EventKind::CiFailed]));
        settings.save(hook(1, &[EventKind::GitChanged]));
        assert_eq!(settings.hooks.len(), 2);
        let wanting: Vec<WebhookId> = settings
            .wanting(EventKind::GitChanged)
            .map(|hook| hook.id)
            .collect();
        assert_eq!(wanting, vec![WebhookId(1)]);
        assert!(settings.wants_code_host());
        assert!(settings.remove(WebhookId(2)).is_some());
        assert!(!settings.wants_code_host());
        assert_eq!(settings.remove(WebhookId(2)), None);
    }

    #[test]
    fn an_id_is_never_issued_twice_even_after_its_hook_is_removed() -> serde_json::Result<()> {
        let mut settings = WebhookSettings::default();
        assert!(settings.is_empty());
        let first = settings.issue_id();
        assert_eq!(first, WebhookId(1));
        assert_eq!(settings.issue_id(), WebhookId(2));
        settings.save(hook(first.0, &[EventKind::CiFailed]));
        assert!(settings.remove(first).is_some());
        assert!(!settings.is_empty());
        let mut reread: WebhookSettings = serde_json::from_str(&serde_json::to_string(&settings)?)?;
        assert_eq!(reread.issue_id(), WebhookId(3));
        let mut imported: WebhookSettings = serde_json::from_str(
            r#"{"hooks":[{"id":9,"name":"ci","url":"https://hooks.example.test/krad","signing":"signed"}]}"#,
        )?;
        assert_eq!(imported.hooks[0].signing, WebhookSigning::Signed);
        assert_eq!(imported.issue_id(), WebhookId(10));
        Ok(())
    }

    #[test]
    fn only_a_test_reaches_a_hook_with_a_ping() {
        let settings = WebhookSettings {
            hooks: vec![hook(1, &[])],
            ..WebhookSettings::default()
        };
        assert_eq!(settings.wanting(EventKind::Ping).count(), 0);
        assert!(settings.hooks[0].wants(EventKind::Ping));
    }
}
