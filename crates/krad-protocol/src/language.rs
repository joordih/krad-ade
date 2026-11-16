use krad_domain::EnvironmentName;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::editor::BufferId;

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TextPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TextRange {
    pub start: TextPosition,
    pub end: TextPosition,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LanguageCommand {
    pub buffer: BufferId,
    pub ask: LanguageAsk,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum LanguageAsk {
    Completion {
        at: TextPosition,
    },
    Hover {
        at: TextPosition,
    },
    Definition {
        at: TextPosition,
    },
    References {
        at: TextPosition,
    },
    SignatureHelp {
        at: TextPosition,
    },
    Rename {
        at: TextPosition,
        new_name: String,
    },
    Format,
    InstallServer,
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Completion {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    pub insert: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<TextRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<u32>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CodeLocation {
    pub path: PathBuf,
    pub range: TextRange,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Signature {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_parameter: Option<u32>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RangeEdit {
    pub range: TextRange,
    pub text: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FileEdits {
    pub path: PathBuf,
    pub edits: Vec<RangeEdit>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LanguageAnswer {
    Completions {
        items: Vec<Completion>,
    },
    Hover {
        markdown: Option<String>,
    },
    Locations {
        locations: Vec<CodeLocation>,
    },
    Signatures {
        signatures: Vec<Signature>,
        active: Option<u32>,
    },
    Edits {
        files: Vec<FileEdits>,
    },
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CodeDiagnostic {
    pub range: TextRange,
    pub severity: DiagnosticSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FileDiagnostics {
    pub environment: EnvironmentName,
    pub path: PathBuf,
    pub diagnostics: Vec<CodeDiagnostic>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ServerState {
    Locating,
    NotInstalled,
    NeedsServerRuntime {
        name: String,
    },
    Installing,
    Starting,
    Running,
    Failed {
        reason: String,
    },
    Stopped,
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ServerShown {
    pub environment: EnvironmentName,
    pub root: PathBuf,
    pub package: String,
    pub state: ServerState,
}
