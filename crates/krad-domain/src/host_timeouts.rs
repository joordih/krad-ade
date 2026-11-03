use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_CONNECT: Duration = Duration::from_secs(15);
const DEFAULT_COMMAND: Duration = Duration::from_secs(60);
const DEFAULT_REQUEST: Duration = Duration::from_secs(30);
const DEFAULT_FORWARD: Duration = Duration::from_secs(10);
const MIN_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_TIMEOUT: Duration = Duration::from_hours(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(from = "u64", into = "u64")]
pub struct TimeoutSecs(Duration);

impl TimeoutSecs {
    #[must_use]
    pub fn new(duration: Duration) -> Self {
        Self(duration.clamp(MIN_TIMEOUT, MAX_TIMEOUT))
    }

    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}

impl From<u64> for TimeoutSecs {
    fn from(secs: u64) -> Self {
        Self::new(Duration::from_secs(secs))
    }
}

impl From<TimeoutSecs> for u64 {
    fn from(timeout: TimeoutSecs) -> Self {
        timeout.0.as_secs()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HostTimeouts {
    #[serde(default = "default_connect")]
    pub connect: TimeoutSecs,
    #[serde(default = "default_command")]
    pub command: TimeoutSecs,
    #[serde(default = "default_request")]
    pub request: TimeoutSecs,
    #[serde(default = "default_forward")]
    pub forward: TimeoutSecs,
}

impl HostTimeouts {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for HostTimeouts {
    fn default() -> Self {
        Self {
            connect: default_connect(),
            command: default_command(),
            request: default_request(),
            forward: default_forward(),
        }
    }
}

fn default_connect() -> TimeoutSecs {
    TimeoutSecs::new(DEFAULT_CONNECT)
}

fn default_command() -> TimeoutSecs {
    TimeoutSecs::new(DEFAULT_COMMAND)
}

fn default_request() -> TimeoutSecs {
    TimeoutSecs::new(DEFAULT_REQUEST)
}

fn default_forward() -> TimeoutSecs {
    TimeoutSecs::new(DEFAULT_FORWARD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_their_defaults_and_values_are_clamped() -> serde_json::Result<()> {
        let timeouts: HostTimeouts = serde_json::from_str(r#"{"connect":5,"request":0}"#)?;
        assert_eq!(timeouts.connect.duration(), Duration::from_secs(5));
        assert_eq!(timeouts.request.duration(), MIN_TIMEOUT);
        assert_eq!(timeouts.command, HostTimeouts::default().command);
        assert_eq!(serde_json::to_string(&TimeoutSecs::from(7))?, "7");
        Ok(())
    }
}
