use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ProblemSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum ProblemLens {
    Off,
    Errors,
    #[default]
    ErrorsAndWarnings,
    All,
}

impl ProblemLens {
    pub const ALL: [Self; 4] = [Self::Off, Self::Errors, Self::ErrorsAndWarnings, Self::All];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Errors => "Errors",
            Self::ErrorsAndWarnings => "Errors and warnings",
            Self::All => "All",
        }
    }

    #[must_use]
    pub const fn shows(self, severity: ProblemSeverity) -> bool {
        match self {
            Self::Off => false,
            Self::Errors => matches!(severity, ProblemSeverity::Error),
            Self::ErrorsAndWarnings => {
                matches!(severity, ProblemSeverity::Error | ProblemSeverity::Warning)
            }
            Self::All => true,
        }
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Minimap {
    #[default]
    Shown,
    Hidden,
}

impl Minimap {
    pub const ALL: [Self; 2] = [Self::Shown, Self::Hidden];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Shown => "On",
            Self::Hidden => "Off",
        }
    }

    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Shown => Self::Hidden,
            Self::Hidden => Self::Shown,
        }
    }

    #[must_use]
    pub const fn is_shown(self) -> bool {
        matches!(self, Self::Shown)
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(from = "u32", into = "u32")]
pub struct ProjectCheckLimit(u32);

impl ProjectCheckLimit {
    pub const MIN: u32 = 100;
    pub const MAX: u32 = 50_000;
    pub const DEFAULT: u32 = 2_000;
    pub const PRESETS: [Self; 4] = [Self(500), Self(Self::DEFAULT), Self(5_000), Self(10_000)];

    #[must_use]
    pub const fn new(files: u32) -> Self {
        if files < Self::MIN {
            Self(Self::MIN)
        } else if files > Self::MAX {
            Self(Self::MAX)
        } else {
            Self(files)
        }
    }

    #[must_use]
    pub const fn files(self) -> u32 {
        self.0
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for ProjectCheckLimit {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

impl From<u32> for ProjectCheckLimit {
    fn from(files: u32) -> Self {
        Self::new(files)
    }
}

impl From<ProjectCheckLimit> for u32 {
    fn from(limit: ProjectCheckLimit) -> Self {
        limit.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct EditorSettings {
    #[serde(default, skip_serializing_if = "ProblemLens::is_default")]
    pub problem_lens: ProblemLens,
    #[serde(default, skip_serializing_if = "Minimap::is_default")]
    pub minimap: Minimap,
    #[serde(default, skip_serializing_if = "ProjectCheckLimit::is_default")]
    pub project_check_limit: ProjectCheckLimit,
}

impl EditorSettings {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_lens_threshold_shows_its_severities_and_the_more_severe_ones() {
        let shown = |lens: ProblemLens| {
            [
                ProblemSeverity::Error,
                ProblemSeverity::Warning,
                ProblemSeverity::Info,
                ProblemSeverity::Hint,
            ]
            .into_iter()
            .filter(|severity| lens.shows(*severity))
            .count()
        };
        assert_eq!(shown(ProblemLens::Off), 0);
        assert_eq!(shown(ProblemLens::Errors), 1);
        assert_eq!(shown(ProblemLens::ErrorsAndWarnings), 2);
        assert_eq!(shown(ProblemLens::All), 4);
    }

    #[test]
    fn a_project_check_limit_stays_within_its_bounds_when_read() -> serde_json::Result<()> {
        let read: ProjectCheckLimit = serde_json::from_str("3")?;
        assert_eq!(read.files(), ProjectCheckLimit::MIN);
        let read: ProjectCheckLimit = serde_json::from_str("9999999")?;
        assert_eq!(read.files(), ProjectCheckLimit::MAX);
        Ok(())
    }

    #[test]
    fn default_editor_settings_are_left_out_of_the_file() -> serde_json::Result<()> {
        assert_eq!(serde_json::to_string(&EditorSettings::default())?, "{}");
        let read: EditorSettings = serde_json::from_str(r#"{"minimap":"Hidden"}"#)?;
        assert_eq!(read.minimap, Minimap::Hidden);
        assert_eq!(read.problem_lens, ProblemLens::ErrorsAndWarnings);
        Ok(())
    }
}
