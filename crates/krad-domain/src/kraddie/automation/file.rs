use super::{Automation, AutomationError, MAX_AUTOMATIONS};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const AUTOMATIONS_FORMAT_VERSION: AutomationFormatVersion = AutomationFormatVersion(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AutomationFormatVersion(pub u32);

#[derive(Debug, PartialEq, Eq)]
pub enum AutomationsFileError {
    Unreadable(String),
    NewerFormat(AutomationFormatVersion),
    RepeatedId,
    TooMany,
    Automation(AutomationError),
}

impl fmt::Display for AutomationsFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(reason) => write!(f, "Not an automations file: {reason}"),
            Self::NewerFormat(version) => write!(
                f,
                "The automations file uses format {}, newer than this app reads ({})",
                version.0, AUTOMATIONS_FORMAT_VERSION.0
            ),
            Self::RepeatedId => f.write_str("Two automations share an id"),
            Self::TooMany => write!(f, "A Kraddie has at most {MAX_AUTOMATIONS} automations"),
            Self::Automation(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for AutomationsFileError {}

impl From<AutomationError> for AutomationsFileError {
    fn from(error: AutomationError) -> Self {
        Self::Automation(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationsFile {
    pub format: AutomationFormatVersion,
    pub automations: Vec<Automation>,
}

#[derive(Deserialize)]
struct VersionOnly {
    format: AutomationFormatVersion,
}

impl AutomationsFile {
    #[must_use]
    pub const fn new(automations: Vec<Automation>) -> Self {
        Self {
            format: AUTOMATIONS_FORMAT_VERSION,
            automations,
        }
    }

    /// # Errors
    /// Fails when the automations cannot be serialized.
    pub fn write(&self) -> Result<String, AutomationsFileError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| AutomationsFileError::Unreadable(error.to_string()))
    }

    /// # Errors
    /// Fails when the text is not an automations file, is newer than this app reads, or one
    /// of its automations is not valid.
    pub fn read(text: &str) -> Result<Vec<Automation>, AutomationsFileError> {
        let unreadable =
            |error: serde_json::Error| AutomationsFileError::Unreadable(error.to_string());
        let version: VersionOnly = serde_json::from_str(text).map_err(unreadable)?;
        if version.format > AUTOMATIONS_FORMAT_VERSION {
            return Err(AutomationsFileError::NewerFormat(version.format));
        }
        let file: Self = serde_json::from_str(text).map_err(unreadable)?;
        validate_automations(&file.automations)?;
        Ok(file.automations)
    }
}

/// # Errors
/// Fails when two automations share an id, there are too many, or one is not valid.
pub fn validate_automations(automations: &[Automation]) -> Result<(), AutomationsFileError> {
    if automations.len() > MAX_AUTOMATIONS {
        return Err(AutomationsFileError::TooMany);
    }
    let repeated = automations.iter().enumerate().any(|(at, automation)| {
        automations[..at]
            .iter()
            .any(|seen| seen.id == automation.id)
    });
    if repeated {
        return Err(AutomationsFileError::RepeatedId);
    }
    for automation in automations {
        automation.validate()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{AutomationId, Trigger};
    use super::*;

    fn automation(id: u32) -> Automation {
        let mut automation = Automation::new(
            AutomationId(id),
            "Find bugs".parse().unwrap_or_else(|_| unreachable!()),
        );
        automation.triggers = vec![Trigger::Manual];
        automation
    }

    #[test]
    fn the_file_round_trips_and_refuses_newer_formats_and_shared_ids() {
        let text = AutomationsFile::new(vec![automation(1), automation(2)])
            .write()
            .unwrap_or_else(|_| unreachable!());
        assert!(text.contains("\"format\": 1"), "{text}");
        assert_eq!(AutomationsFile::read(&text).map(|read| read.len()), Ok(2));
        let newer = text.replace("\"format\": 1", "\"format\": 9");
        assert_eq!(
            AutomationsFile::read(&newer),
            Err(AutomationsFileError::NewerFormat(AutomationFormatVersion(
                9
            )))
        );
        let shared = AutomationsFile::new(vec![automation(1), automation(1)])
            .write()
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            AutomationsFile::read(&shared),
            Err(AutomationsFileError::RepeatedId)
        );
        assert!(matches!(
            AutomationsFile::read("[]"),
            Err(AutomationsFileError::Unreadable(_))
        ));
    }
}
