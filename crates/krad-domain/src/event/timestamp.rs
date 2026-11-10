use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Timestamp(DateTime<Utc>);

impl Timestamp {
    #[must_use]
    pub fn now() -> Self {
        Self::from(SystemTime::now())
    }
}

impl From<SystemTime> for Timestamp {
    fn from(at: SystemTime) -> Self {
        Self(DateTime::from(at))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.to_rfc3339_opts(SecondsFormat::Millis, true))
    }
}

impl From<Timestamp> for String {
    fn from(at: Timestamp) -> Self {
        at.to_string()
    }
}

impl TryFrom<String> for Timestamp {
    type Error = chrono::ParseError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        DateTime::parse_from_rfc3339(&text).map(|at| Self(at.with_timezone(&Utc)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    const A_DAY_AND_A_BIT: Duration = Duration::from_millis(86_400_123);

    #[test]
    fn timestamps_travel_as_rfc_3339_in_utc_with_milliseconds() -> Result<(), serde_json::Error> {
        let at = Timestamp::from(UNIX_EPOCH + A_DAY_AND_A_BIT);
        let line = serde_json::to_string(&at)?;
        assert_eq!(line, "\"1970-01-02T00:00:00.123Z\"");
        assert_eq!(serde_json::from_str::<Timestamp>(&line)?, at);
        let offset = serde_json::from_str::<Timestamp>("\"1970-01-02T02:00:00.123+02:00\"")?;
        assert_eq!(offset, at);
        assert!(serde_json::from_str::<Timestamp>("\"yesterday\"").is_err());
        Ok(())
    }
}
