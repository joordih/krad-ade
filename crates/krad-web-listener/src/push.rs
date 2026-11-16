mod reach;
mod seal;
mod vapid;
mod watch;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::SystemRandom;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;
use ureq::Agent;

use self::vapid::{origin_of, Vapid, VapidError};
pub(crate) use self::watch::watch;

const BOOK_FILE: &str = "web-push.json";
const MAX_SUBSCRIPTIONS: usize = 16;
const TIME_TO_LIVE: &str = "86400";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(20);
const GONE: [u16; 2] = [404, 410];
const PUSH_SERVICES: [&str; 6] = [
    ".push.apple.com",
    "fcm.googleapis.com",
    "android.googleapis.com",
    ".push.services.mozilla.com",
    ".notify.windows.com",
    "push.services.mozilla.com",
];

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub(crate) struct Subscription {
    pub(crate) endpoint: String,
    pub(crate) keys: SubscriptionKeys,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub(crate) struct SubscriptionKeys {
    pub(crate) p256dh: String,
    pub(crate) auth: String,
}

#[derive(Serialize)]
pub(crate) struct Notice {
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) tag: String,
    pub(crate) url: String,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PushError {
    #[error(transparent)]
    Vapid(#[from] VapidError),
    #[error("the push subscriptions could not be read or written")]
    Io(#[from] io::Error),
    #[error("the push subscriptions are not valid")]
    Json(#[from] serde_json::Error),
    #[error("that is not a known push service")]
    UnknownService,
    #[error("this runtime keeps at most {MAX_SUBSCRIPTIONS} push subscriptions")]
    Full,
}

pub(crate) struct Push {
    root: PathBuf,
    vapid: Vapid,
    random: SystemRandom,
    agent: Agent,
    book: Mutex<Vec<Subscription>>,
}

fn known_service(endpoint: &str) -> bool {
    origin_of(endpoint)
        .and_then(|origin| origin.split("://").nth(1))
        .map(|host| host.split(':').next().unwrap_or(host))
        .is_some_and(|host| {
            PUSH_SERVICES.iter().any(|service| {
                host == service.trim_start_matches('.')
                    || (service.starts_with('.') && host.ends_with(service))
            })
        })
}

fn read_book(path: &Path) -> Result<Vec<Subscription>, PushError> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

enum Sent {
    Delivered,
    Gone,
    Failed,
}

impl Push {
    pub(crate) fn open(root: &Path) -> Result<Self, PushError> {
        let random = SystemRandom::new();
        let vapid = Vapid::open(root, &random)?;
        let book = read_book(&root.join(BOOK_FILE))?;
        let agent = Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_per_read(Some(READ_TIMEOUT))
            .http_status_as_error(false)
            .build()
            .new_agent();
        Ok(Self {
            root: root.to_owned(),
            vapid,
            random,
            agent,
            book: Mutex::new(book),
        })
    }

    pub(crate) fn public_key(&self) -> String {
        self.vapid.public_key()
    }

    fn keep(&self, book: &[Subscription]) -> Result<(), PushError> {
        fs::write(self.root.join(BOOK_FILE), serde_json::to_vec_pretty(book)?)?;
        Ok(())
    }

    pub(crate) fn subscribe(&self, subscription: Subscription) -> Result<(), PushError> {
        if !known_service(&subscription.endpoint) {
            return Err(PushError::UnknownService);
        }
        let mut book = self.book.lock().unwrap_or_else(PoisonError::into_inner);
        book.retain(|known| known.endpoint != subscription.endpoint);
        if book.len() >= MAX_SUBSCRIPTIONS {
            return Err(PushError::Full);
        }
        book.push(subscription);
        self.keep(&book)
    }

    pub(crate) fn unsubscribe(&self, endpoint: &str) -> Result<(), PushError> {
        let mut book = self.book.lock().unwrap_or_else(PoisonError::into_inner);
        book.retain(|known| known.endpoint != endpoint);
        self.keep(&book)
    }

    fn send(&self, subscription: &Subscription, payload: &[u8]) -> Sent {
        let decoded = (
            URL_SAFE_NO_PAD.decode(subscription.keys.p256dh.trim_end_matches('=')),
            URL_SAFE_NO_PAD.decode(subscription.keys.auth.trim_end_matches('=')),
        );
        let (Ok(receiver), Ok(auth)) = decoded else {
            return Sent::Gone;
        };
        let Some(audience) = origin_of(&subscription.endpoint) else {
            return Sent::Gone;
        };
        let sealed = match seal::seal(&receiver, &auth, payload, &self.random) {
            Ok(sealed) => sealed,
            Err(seal::SealError::Keys) => return Sent::Gone,
            Err(error) => {
                tracing::warn!(%error, "a push notification could not be sealed");
                return Sent::Failed;
            }
        };
        let authorization = match self.vapid.authorization(audience, &self.random) {
            Ok(authorization) => authorization,
            Err(error) => {
                tracing::warn!(%error, "a push notification could not be signed");
                return Sent::Failed;
            }
        };
        let response = self
            .agent
            .post(&subscription.endpoint)
            .header("Authorization", &authorization)
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream")
            .header("TTL", TIME_TO_LIVE)
            .header("Urgency", "high")
            .send(&sealed[..]);
        match response {
            Ok(response) if response.status().is_success() => Sent::Delivered,
            Ok(response) if GONE.contains(&response.status().as_u16()) => {
                tracing::debug!(status = %response.status(), "a push subscription lapsed");
                Sent::Gone
            }
            Ok(response) => {
                tracing::warn!(status = %response.status(), "a push service refused a notification");
                Sent::Failed
            }
            Err(error) => {
                tracing::debug!(%error, "a push service could not be reached");
                Sent::Failed
            }
        }
    }

    pub(crate) fn notify(&self, notice: &Notice) {
        let Ok(payload) = serde_json::to_vec(notice) else {
            return;
        };
        let book = self
            .book
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let gone: Vec<String> = book
            .iter()
            .filter(|subscription| matches!(self.send(subscription, &payload), Sent::Gone))
            .map(|subscription| subscription.endpoint.clone())
            .collect();
        for endpoint in gone {
            if let Err(error) = self.unsubscribe(&endpoint) {
                tracing::warn!(%error, "a lapsed push subscription could not be forgotten");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_push_services_are_kept() {
        assert!(known_service("https://web.push.apple.com/QGx4"));
        assert!(known_service("https://fcm.googleapis.com/fcm/send/abc"));
        assert!(known_service(
            "https://updates.push.services.mozilla.com/wpush/v2/x"
        ));
        assert!(!known_service("https://evil.example/web.push.apple.com"));
        assert!(!known_service("https://push.apple.com.evil.example/x"));
        assert!(!known_service("http://web.push.apple.com/x"));
        assert!(!known_service("https://100.64.0.1/push"));
    }
}
