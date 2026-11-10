use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::agent::AgentPreset;
use crate::NotAVersion;

const VERSION_SEPARATOR: char = '.';
const VERSION_PREFIX: char = 'v';
const PRE_RELEASE_SEPARATORS: [char; 2] = ['-', '+'];
const WORD_BREAKS: [char; 4] = ['(', ')', ',', ';'];
const WINDOWS_EXECUTABLE_SUFFIX: &str = ".exe";

const CLAUDE_MINIMUM: HarnessVersion = HarnessVersion::new(2, 1, 41);
const CODEX_MINIMUM: HarnessVersion = HarnessVersion::new(0, 40, 0);
const OPENCODE_MINIMUM: HarnessVersion = HarnessVersion::new(1, 0, 0);

const USER_BIN_FOLDERS: [&str; 3] = [".local/bin", ".npm-global/bin", ".bun/bin"];
const SYSTEM_BIN_FOLDERS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];
const CLAUDE_OWN_FOLDERS: &[&str] = &[".claude/local"];
const CODEX_OWN_FOLDERS: &[&str] = &[];
const OPENCODE_OWN_FOLDERS: &[&str] = &[".opencode/bin"];

const CLAUDE_SETUP: HarnessSetup = HarnessSetup {
    minimum: CLAUDE_MINIMUM,
    install: "npm install -g @anthropic-ai/claude-code",
    update: "claude update",
    sign_in: "claude auth login",
    own_folders: CLAUDE_OWN_FOLDERS,
};

const CODEX_SETUP: HarnessSetup = HarnessSetup {
    minimum: CODEX_MINIMUM,
    install: "npm install -g @openai/codex",
    update: "npm install -g @openai/codex@latest",
    sign_in: "codex login",
    own_folders: CODEX_OWN_FOLDERS,
};

const OPENCODE_SETUP: HarnessSetup = HarnessSetup {
    minimum: OPENCODE_MINIMUM,
    install: "npm install -g opencode-ai",
    update: "opencode upgrade",
    sign_in: "opencode auth login",
    own_folders: OPENCODE_OWN_FOLDERS,
};

pub const CHECKED_HARNESSES: [AgentPreset; 3] = [
    AgentPreset::ClaudeCode,
    AgentPreset::Codex,
    AgentPreset::Opencode,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HarnessVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl HarnessVersion {
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    #[must_use]
    pub fn find_in(output: &str) -> Option<Self> {
        output
            .split(|c: char| c.is_whitespace() || WORD_BREAKS.contains(&c))
            .find_map(|word| word.parse().ok())
    }
}

impl FromStr for HarnessVersion {
    type Err = NotAVersion;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bare = text.strip_prefix(VERSION_PREFIX).unwrap_or(text);
        let release = bare
            .split(PRE_RELEASE_SEPARATORS)
            .next()
            .ok_or(NotAVersion)?;
        let parts: Vec<u32> = release
            .split(VERSION_SEPARATOR)
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| NotAVersion)?;
        match parts[..] {
            [major, minor] => Ok(Self::new(major, minor, 0)),
            [major, minor, patch] => Ok(Self::new(major, minor, patch)),
            _ => Err(NotAVersion),
        }
    }
}

impl fmt::Display for HarnessVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessSetup {
    pub minimum: HarnessVersion,
    pub install: &'static str,
    pub update: &'static str,
    pub sign_in: &'static str,
    own_folders: &'static [&'static str],
}

impl HarnessSetup {
    #[must_use]
    pub fn off_path_candidates(&self, program: &str, home: Option<&Path>) -> Vec<PathBuf> {
        let user = home.into_iter().flat_map(|home| {
            USER_BIN_FOLDERS
                .iter()
                .chain(self.own_folders)
                .map(move |folder| home.join(folder))
        });
        let system = SYSTEM_BIN_FOLDERS.iter().map(PathBuf::from);
        user.chain(system)
            .flat_map(|folder| {
                [
                    folder.join(program),
                    folder.join(format!("{program}{WINDOWS_EXECUTABLE_SUFFIX}")),
                ]
            })
            .collect()
    }
}

impl AgentPreset {
    #[must_use]
    pub const fn setup(self) -> Option<HarnessSetup> {
        match self {
            Self::ClaudeCode => Some(CLAUDE_SETUP),
            Self::Codex => Some(CODEX_SETUP),
            Self::Opencode => Some(OPENCODE_SETUP),
            Self::Shell | Self::Gateway | Self::Acp(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignIn {
    SignedIn,
    SignedOut,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HarnessHealth {
    Missing,
    OffPath {
        program: PathBuf,
    },
    Found {
        version: Option<HarnessVersion>,
        sign_in: SignIn,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HealthIssue {
    NotInstalled,
    NotOnPath {
        program: PathBuf,
    },
    UnknownVersion,
    TooOld {
        found: HarnessVersion,
        minimum: HarnessVersion,
    },
    SignedOut,
    SignInUnknown,
}

impl HealthIssue {
    #[must_use]
    pub const fn blocks(&self) -> bool {
        match self {
            Self::NotInstalled | Self::NotOnPath { .. } | Self::TooOld { .. } | Self::SignedOut => {
                true
            }
            Self::UnknownVersion | Self::SignInUnknown => false,
        }
    }
}

impl HarnessHealth {
    #[must_use]
    pub fn issues(&self, minimum: HarnessVersion) -> Vec<HealthIssue> {
        match self {
            Self::Missing => vec![HealthIssue::NotInstalled],
            Self::OffPath { program } => vec![HealthIssue::NotOnPath {
                program: program.clone(),
            }],
            Self::Found { version, sign_in } => {
                let version = match version {
                    None => Some(HealthIssue::UnknownVersion),
                    Some(found) if *found < minimum => Some(HealthIssue::TooOld {
                        found: *found,
                        minimum,
                    }),
                    Some(_) => None,
                };
                let sign_in = match sign_in {
                    SignIn::SignedIn => None,
                    SignIn::SignedOut => Some(HealthIssue::SignedOut),
                    SignIn::Unknown => Some(HealthIssue::SignInUnknown),
                };
                version.into_iter().chain(sign_in).collect()
            }
        }
    }

    #[must_use]
    pub fn is_ready(&self, minimum: HarnessVersion) -> bool {
        !self.issues(minimum).iter().any(HealthIssue::blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Option<HarnessVersion> {
        HarnessVersion::find_in(text)
    }

    #[test]
    fn the_version_is_read_from_what_each_harness_prints() {
        assert_eq!(
            version("2.1.319 (Claude Code)\n"),
            Some(HarnessVersion::new(2, 1, 319))
        );
        assert_eq!(
            version("codex-cli 0.166.0"),
            Some(HarnessVersion::new(0, 166, 0))
        );
        assert_eq!(
            version("opencode v2.2.6"),
            Some(HarnessVersion::new(2, 2, 6))
        );
        assert_eq!(version("1.4"), Some(HarnessVersion::new(1, 4, 0)));
        assert_eq!(
            version("codex-cli 0.170.0-alpha.3"),
            Some(HarnessVersion::new(0, 170, 0))
        );
    }

    #[test]
    fn output_without_a_version_number_reads_as_none() {
        assert_eq!(version(""), None);
        assert_eq!(version("error: unknown option '--version'"), None);
        assert_eq!(version("1.2.3.4 build"), None);
        assert_eq!(version("v"), None);
    }

    #[test]
    fn versions_compare_by_major_minor_and_patch() {
        assert!(HarnessVersion::new(2, 1, 40) < CLAUDE_MINIMUM);
        assert!(HarnessVersion::new(2, 1, 319) > CLAUDE_MINIMUM);
        assert!(HarnessVersion::new(0, 166, 0) > CODEX_MINIMUM);
        assert_eq!(HarnessVersion::new(2, 2, 6).to_string(), "2.2.6");
    }

    #[test]
    fn a_harness_that_is_not_found_needs_installing() {
        assert_eq!(
            HarnessHealth::Missing.issues(CLAUDE_MINIMUM),
            [HealthIssue::NotInstalled]
        );
        let off_path = HarnessHealth::OffPath {
            program: PathBuf::from("/home/u/.local/bin/claude"),
        };
        assert_eq!(
            off_path.issues(CLAUDE_MINIMUM),
            [HealthIssue::NotOnPath {
                program: PathBuf::from("/home/u/.local/bin/claude")
            }]
        );
        assert!(!off_path.is_ready(CLAUDE_MINIMUM));
    }

    #[test]
    fn an_old_signed_out_harness_reports_both_problems() {
        let health = HarnessHealth::Found {
            version: Some(HarnessVersion::new(2, 0, 1)),
            sign_in: SignIn::SignedOut,
        };
        assert_eq!(
            health.issues(CLAUDE_MINIMUM),
            [
                HealthIssue::TooOld {
                    found: HarnessVersion::new(2, 0, 1),
                    minimum: CLAUDE_MINIMUM
                },
                HealthIssue::SignedOut
            ]
        );
        assert!(!health.is_ready(CLAUDE_MINIMUM));
    }

    #[test]
    fn what_cannot_be_told_is_reported_without_blocking() {
        let health = HarnessHealth::Found {
            version: None,
            sign_in: SignIn::Unknown,
        };
        assert_eq!(
            health.issues(CODEX_MINIMUM),
            [HealthIssue::UnknownVersion, HealthIssue::SignInUnknown]
        );
        assert!(health.is_ready(CODEX_MINIMUM));
    }

    #[test]
    fn a_current_signed_in_harness_is_ready() {
        let health = HarnessHealth::Found {
            version: Some(HarnessVersion::new(0, 166, 0)),
            sign_in: SignIn::SignedIn,
        };
        assert_eq!(health.issues(CODEX_MINIMUM), []);
        assert!(health.is_ready(CODEX_MINIMUM));
    }

    #[test]
    fn only_the_cli_harnesses_have_a_setup() {
        for harness in CHECKED_HARNESSES {
            assert!(harness.setup().is_some(), "{harness:?}");
        }
        assert_eq!(AgentPreset::Shell.setup(), None);
        assert_eq!(AgentPreset::Gateway.setup(), None);
    }

    #[test]
    fn off_path_candidates_look_in_the_home_and_system_folders() {
        let home = Path::new("/home/u");
        let candidates = OPENCODE_SETUP.off_path_candidates("opencode", Some(home));
        assert!(candidates.contains(&home.join(".opencode/bin/opencode")));
        assert!(candidates.contains(&home.join(".local/bin/opencode.exe")));
        assert!(candidates.contains(&PathBuf::from("/opt/homebrew/bin/opencode")));
        let without_home = CODEX_SETUP.off_path_candidates("codex", None);
        assert!(without_home.iter().all(|path| !path.starts_with(home)));
    }
}
