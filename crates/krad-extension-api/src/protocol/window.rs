use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::icon::Icon;
use crate::id::ProgressToken;
use crate::protocol::{notification, request};
use crate::view::{InputKind, Tone};

pub const MAX_PERCENT: u8 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToastParams {
    pub message: String,
    #[serde(default)]
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConfirmParams {
    pub title: String,
    #[serde(default)]
    pub detail: Option<String>,
    pub confirm: String,
    #[serde(default)]
    pub tone: Tone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Confirmed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PickMode {
    #[default]
    One,
    Many,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PickItem {
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub icon: Option<Icon>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PickParams {
    pub title: String,
    #[serde(default)]
    pub placeholder: Option<String>,
    pub items: Vec<PickItem>,
    #[serde(default)]
    pub mode: PickMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InputBoxParams {
    pub title: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub kind: InputKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProgressState {
    Begin {
        title: String,
    },
    Report {
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        percent: Option<u8>,
    },
    End,
}

impl ProgressState {
    #[must_use]
    pub fn bounded(self) -> Self {
        match self {
            Self::Report { message, percent } => Self::Report {
                message,
                percent: percent.map(|percent| percent.min(MAX_PERCENT)),
            },
            other => other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProgressParams {
    pub token: ProgressToken,
    #[serde(flatten)]
    pub state: ProgressState,
}

notification!(Toast, "window/toast", App, ToastParams);
request!(Confirm, "window/confirm", App, ConfirmParams, Answer);
request!(Pick, "window/pick", App, PickParams, Option<Vec<String>>);
request!(
    InputBox,
    "window/input",
    App,
    InputBoxParams,
    Option<String>
);
notification!(Progress, "window/progress", App, ProgressParams);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_never_passes_one_hundred_percent() {
        let report = ProgressState::Report {
            message: None,
            percent: Some(250),
        };
        assert_eq!(
            report.bounded(),
            ProgressState::Report {
                message: None,
                percent: Some(MAX_PERCENT)
            }
        );
    }
}
