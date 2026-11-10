use super::automation::{validate_automations, Automation, AutomationsFileError};
use super::design::DesignError;
use super::legacy::LegacyFile;
use super::Kraddie;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const KRADDIE_FILE_EXTENSION: &str = "kraddie";
pub const LEGACY_KRADDIE_FILE_EXTENSION: &str = "kradpet";
pub const KRADDIE_FORMAT_VERSION: KraddieFormatVersion = KraddieFormatVersion(2);
const FIRST_FORMAT_WITH_AUTOMATIONS: KraddieFormatVersion = KraddieFormatVersion(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KraddieFormatVersion(pub u32);

#[derive(Debug, PartialEq, Eq)]
pub enum KraddieFileError {
    Unreadable(String),
    NewerFormat(KraddieFormatVersion),
    Design(DesignError),
    Automations(AutomationsFileError),
    NotAnAgent,
}

impl fmt::Display for KraddieFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(reason) => write!(f, "Not a Kraddie file: {reason}"),
            Self::NewerFormat(version) => write!(
                f,
                "The Kraddie file uses format {}, newer than this app reads ({})",
                version.0, KRADDIE_FORMAT_VERSION.0
            ),
            Self::Design(error) => error.fmt(f),
            Self::Automations(error) => error.fmt(f),
            Self::NotAnAgent => f.write_str("The Kraddie's harness is not a coding agent"),
        }
    }
}

impl std::error::Error for KraddieFileError {}

impl From<DesignError> for KraddieFileError {
    fn from(error: DesignError) -> Self {
        Self::Design(error)
    }
}

impl From<AutomationsFileError> for KraddieFileError {
    fn from(error: AutomationsFileError) -> Self {
        Self::Automations(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KraddieFile {
    pub format: KraddieFormatVersion,
    #[serde(alias = "pet")]
    pub kraddie: Kraddie,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub automations: Vec<Automation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadKraddie {
    pub kraddie: Kraddie,
    pub automations: Vec<Automation>,
    pub migrated: bool,
}

#[derive(Deserialize)]
struct VersionOnly {
    format: KraddieFormatVersion,
}

impl KraddieFile {
    #[must_use]
    pub const fn new(kraddie: Kraddie, automations: Vec<Automation>) -> Self {
        Self {
            format: KRADDIE_FORMAT_VERSION,
            kraddie,
            automations,
        }
    }

    /// # Errors
    /// Fails when the kraddie cannot be serialized.
    pub fn write(&self) -> Result<String, KraddieFileError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| KraddieFileError::Unreadable(error.to_string()))
    }

    /// # Errors
    /// Fails when the text is not a kraddie file, is newer than this app reads, or its kraddie or
    /// automations are not valid. A file of the first format becomes a kraddie with one
    /// automation holding its old instructions and triggers.
    pub fn read(text: &str) -> Result<ReadKraddie, KraddieFileError> {
        let unreadable = |error: serde_json::Error| KraddieFileError::Unreadable(error.to_string());
        let version: VersionOnly = serde_json::from_str(text).map_err(unreadable)?;
        if version.format > KRADDIE_FORMAT_VERSION {
            return Err(KraddieFileError::NewerFormat(version.format));
        }
        let file: Self = serde_json::from_str(text).map_err(unreadable)?;
        let migrated = version.format < FIRST_FORMAT_WITH_AUTOMATIONS;
        let automations = if migrated {
            let legacy: LegacyFile = serde_json::from_str(text).map_err(unreadable)?;
            legacy
                .kraddie
                .into_automation(&file.kraddie.name)
                .into_iter()
                .collect()
        } else {
            file.automations
        };
        validate(&file.kraddie)?;
        validate_automations(&automations)?;
        Ok(ReadKraddie {
            kraddie: file.kraddie,
            automations,
            migrated,
        })
    }
}

fn validate(kraddie: &Kraddie) -> Result<(), KraddieFileError> {
    kraddie.design.validate()?;
    if !kraddie.harness.is_agent() {
        return Err(KraddieFileError::NotAnAgent);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
