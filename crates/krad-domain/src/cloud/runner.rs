use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use super::CloudProcessId;
use crate::SshDestination;

const NAME_MAX_CHARS: usize = 64;
const DEFAULT_MAX_SESSIONS: u8 = 4;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct CloudRunnerId(pub u64);

impl fmt::Display for CloudRunnerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudRunnerNameError {
    Empty,
    TooLong,
}

impl fmt::Display for CloudRunnerNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("Enter a name for the cloud runner"),
            Self::TooLong => write!(formatter, "Keep the name to {NAME_MAX_CHARS} characters"),
        }
    }
}

impl std::error::Error for CloudRunnerNameError {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CloudRunnerName(String);

impl CloudRunnerName {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for CloudRunnerName {
    type Err = CloudRunnerNameError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.is_empty() {
            return Err(CloudRunnerNameError::Empty);
        }
        if text.chars().count() > NAME_MAX_CHARS {
            return Err(CloudRunnerNameError::TooLong);
        }
        Ok(Self(text.to_owned()))
    }
}

impl TryFrom<String> for CloudRunnerName {
    type Error = CloudRunnerNameError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<CloudRunnerName> for String {
    fn from(name: CloudRunnerName) -> Self {
        name.0
    }
}

impl fmt::Display for CloudRunnerName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudRunner {
    pub id: CloudRunnerId,
    pub name: CloudRunnerName,
    pub destination: SshDestination,
    #[serde(default, skip_serializing_if = "OfflineWebhooks::is_off")]
    pub offline_webhooks: OfflineWebhooks,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfflineWebhooks {
    #[default]
    Off,
    Send,
}

impl OfflineWebhooks {
    #[must_use]
    pub fn is_off(&self) -> bool {
        *self == Self::Off
    }
}

const fn default_max_sessions() -> u8 {
    DEFAULT_MAX_SESSIONS
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudStop {
    pub runner: CloudRunnerId,
    pub process: CloudProcessId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudSettings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runners: Vec<CloudRunner>,
    #[serde(default = "default_max_sessions")]
    pub max_sessions: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_id: Option<CloudRunnerId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    stopping: Vec<CloudStop>,
}

impl Default for CloudSettings {
    fn default() -> Self {
        Self {
            runners: Vec::new(),
            max_sessions: DEFAULT_MAX_SESSIONS,
            last_id: None,
            stopping: Vec::new(),
        }
    }
}

impl CloudSettings {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn add(&mut self, name: CloudRunnerName, destination: SshDestination) -> CloudRunnerId {
        let highest = self
            .runners
            .iter()
            .map(|runner| runner.id)
            .chain(self.last_id)
            .max()
            .unwrap_or_default();
        let id = CloudRunnerId(highest.0.saturating_add(1));
        self.last_id = Some(id);
        self.runners.push(CloudRunner {
            id,
            name,
            destination,
            offline_webhooks: OfflineWebhooks::Off,
        });
        id
    }

    pub fn remove(&mut self, id: CloudRunnerId) -> Option<CloudRunner> {
        let index = self.runners.iter().position(|runner| runner.id == id)?;
        self.stopping.retain(|stop| stop.runner != id);
        Some(self.runners.remove(index))
    }

    pub fn queue_stop(&mut self, runner: CloudRunnerId, process: CloudProcessId) {
        let stop = CloudStop { runner, process };
        if self.find(runner).is_some() && !self.stopping.contains(&stop) {
            self.stopping.push(stop);
        }
    }

    pub fn take_stops(&mut self, runner: CloudRunnerId) -> Vec<CloudProcessId> {
        let (taken, kept) = std::mem::take(&mut self.stopping)
            .into_iter()
            .partition(|stop| stop.runner == runner);
        self.stopping = kept;
        taken
            .into_iter()
            .map(|stop: CloudStop| stop.process)
            .collect()
    }

    #[must_use]
    pub fn find(&self, id: CloudRunnerId) -> Option<&CloudRunner> {
        self.runners.iter().find(|runner| runner.id == id)
    }

    pub fn set_offline_webhooks(&mut self, id: CloudRunnerId, wanted: OfflineWebhooks) -> bool {
        let Some(runner) = self.runners.iter_mut().find(|runner| runner.id == id) else {
            return false;
        };
        let changed = runner.offline_webhooks != wanted;
        runner.offline_webhooks = wanted;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn runners_round_trip_and_ids_are_never_reused() -> TestResult {
        let mut cloud = CloudSettings::default();
        let first = cloud.add("Build box".parse()?, "me@build:2200".parse()?);
        let second = cloud.add("GPU".parse()?, "gpu".parse()?);
        assert_eq!((first, second), (CloudRunnerId(1), CloudRunnerId(2)));
        assert!(cloud.remove(second).is_some());
        assert_eq!(
            cloud.add("Again".parse()?, "gpu".parse()?),
            CloudRunnerId(3)
        );
        let text = serde_json::to_string(&cloud)?;
        assert!(!text.contains("offline_webhooks"));
        assert_eq!(serde_json::from_str::<CloudSettings>(&text)?, cloud);
        assert!(cloud.set_offline_webhooks(first, OfflineWebhooks::Send));
        assert!(!cloud.set_offline_webhooks(first, OfflineWebhooks::Send));
        assert!(!cloud.set_offline_webhooks(CloudRunnerId(99), OfflineWebhooks::Send));
        let text = serde_json::to_string(&cloud)?;
        assert!(text.contains(r#""offline_webhooks":"send""#));
        assert_eq!(serde_json::from_str::<CloudSettings>(&text)?, cloud);
        assert_eq!(
            cloud
                .find(first)
                .map(|runner| runner.destination.to_string()),
            Some("me@build:2200".to_owned())
        );
        Ok(())
    }

    #[test]
    fn the_defaults_allow_four_sessions_and_are_left_out() -> TestResult {
        let cloud: CloudSettings = serde_json::from_str("{}")?;
        assert_eq!(cloud.max_sessions, 4);
        assert!(cloud.is_default());
        assert_eq!(serde_json::to_string(&cloud)?, r#"{"max_sessions":4}"#);
        let settings = serde_json::to_value(crate::Settings::default())?;
        assert!(settings.get("cloud").is_none());
        let mut with_runner = crate::Settings::default();
        with_runner.cloud.add("Box".parse()?, "box".parse()?);
        let text = serde_json::to_string(&with_runner)?;
        assert_eq!(
            serde_json::from_str::<crate::Settings>(&text)?.cloud,
            with_runner.cloud
        );
        Ok(())
    }

    #[test]
    fn stops_wait_for_their_runner_and_leave_with_it() -> TestResult {
        let mut cloud = CloudSettings::default();
        let gpu = cloud.add("GPU".parse()?, "gpu".parse()?);
        let box_id = cloud.add("Box".parse()?, "box".parse()?);
        let chat: CloudProcessId = "api-a1-7-f00dbe".parse()?;
        cloud.queue_stop(gpu, chat.clone());
        cloud.queue_stop(gpu, chat.clone());
        cloud.queue_stop(box_id, "api-a1-8-beef00".parse()?);
        cloud.queue_stop(CloudRunnerId(99), "gone-1-000000".parse()?);
        let text = serde_json::to_string(&cloud)?;
        assert_eq!(serde_json::from_str::<CloudSettings>(&text)?, cloud);
        assert_eq!(cloud.take_stops(gpu), vec![chat]);
        assert_eq!(cloud.take_stops(gpu), Vec::<CloudProcessId>::new());
        cloud.remove(box_id);
        assert_eq!(cloud.take_stops(box_id), Vec::<CloudProcessId>::new());
        Ok(())
    }

    #[test]
    fn a_name_is_trimmed_and_bounded() {
        assert_eq!(
            " Build ".parse::<CloudRunnerName>().map(String::from),
            Ok("Build".to_owned())
        );
        assert_eq!(
            "  ".parse::<CloudRunnerName>(),
            Err(CloudRunnerNameError::Empty)
        );
        assert_eq!(
            "x".repeat(NAME_MAX_CHARS + 1).parse::<CloudRunnerName>(),
            Err(CloudRunnerNameError::TooLong)
        );
    }
}
