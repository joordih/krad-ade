use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::acp::{AcpPermissionKind, AcpPermissionOption, AcpPermissionOptionId};
use crate::attention::AttentionKind;
use crate::chat::{AskOutcome, AskVerdict, ToolCall};
use crate::file_edit::FileEdit;
use crate::permission_mode::PermissionMode;
use crate::tool_kind::ToolKind;

const SUBJECT_SEPARATOR: &str = " ";
const QUESTIONS_SEPARATOR: &str = "; ";

fn each_entry<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let Value::Array(entries) = Value::deserialize(deserializer)? else {
        return Ok(Vec::new());
    };
    Ok(entries
        .into_iter()
        .filter_map(|entry| T::deserialize(entry).ok())
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Question {
    #[serde(rename = "question")]
    pub text: String,
    pub header: Option<String>,
    #[serde(default, deserialize_with = "each_entry")]
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multi_select: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AskKind {
    Permission {
        tool: String,
        kind: ToolKind,
        summary: String,
        title: Option<String>,
        edit: Option<FileEdit>,
    },
    Questions(Vec<Question>),
    Plan {
        plan: String,
    },
    AcpPermission {
        tool: ToolCall,
        options: Vec<AcpPermissionOption>,
    },
}

impl AskKind {
    #[must_use]
    pub fn subject(&self) -> String {
        match self {
            Self::Permission { tool, summary, .. } => format!("{tool}{SUBJECT_SEPARATOR}{summary}")
                .trim()
                .to_owned(),
            Self::AcpPermission { tool, .. } => {
                format!("{}{SUBJECT_SEPARATOR}{}", tool.name, tool.summary)
                    .trim()
                    .to_owned()
            }
            Self::Questions(questions) => questions
                .iter()
                .map(|question| question.text.as_str())
                .collect::<Vec<_>>()
                .join(QUESTIONS_SEPARATOR),
            Self::Plan { plan } => plan
                .lines()
                .map(|line| line.trim_start_matches('#').trim())
                .find(|line| !line.is_empty())
                .unwrap_or_default()
                .to_owned(),
        }
    }

    #[must_use]
    pub fn outcome(&self, verdict: AskVerdict) -> AskOutcome {
        AskOutcome {
            subject: self.subject(),
            verdict,
        }
    }

    #[must_use]
    pub const fn attention(&self) -> AttentionKind {
        match self {
            Self::Permission { .. } | Self::AcpPermission { .. } => AttentionKind::Permission,
            Self::Questions(_) | Self::Plan { .. } => AttentionKind::Ask,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PickedAnswer {
    pub question: String,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AskAnswer {
    Allow,
    AllowAlways,
    ApprovePlan { mode: PermissionMode },
    Deny { message: String },
    Answers(Vec<PickedAnswer>),
    Chosen(AcpPermissionOptionId),
}

impl AskAnswer {
    fn chosen_verdict(&self, kind: &AskKind) -> Option<AskVerdict> {
        let (Self::Chosen(id), AskKind::AcpPermission { options, .. }) = (self, kind) else {
            return None;
        };
        let chosen = options.iter().find(|option| &option.id == id)?;
        Some(match chosen.kind {
            AcpPermissionKind::AllowOnce => AskVerdict::Allowed,
            AcpPermissionKind::AllowAlways => AskVerdict::AllowedAlways,
            AcpPermissionKind::RejectOnce | AcpPermissionKind::RejectAlways => AskVerdict::Denied,
        })
    }

    #[must_use]
    pub fn verdict(&self, kind: &AskKind) -> AskVerdict {
        if let Some(verdict) = self.chosen_verdict(kind) {
            return verdict;
        }
        match (self, kind) {
            (Self::Allow, _) => AskVerdict::Allowed,
            (Self::AllowAlways, _) => AskVerdict::AllowedAlways,
            (Self::ApprovePlan { mode }, _) => AskVerdict::PlanApproved(*mode),
            (Self::Deny { .. }, AskKind::Plan { .. }) => AskVerdict::PlanRejected,
            (Self::Deny { .. }, _) => AskVerdict::Denied,
            (Self::Answers(_) | Self::Chosen(_), _) => AskVerdict::Answered,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_reads_its_options_and_skips_broken_ones() -> serde_json::Result<()> {
        let question: Question = serde_json::from_str(
            r#"{"question":"Which?","header":null,"options":[{"label":"A","description":null},7],"multiSelect":true}"#,
        )?;
        assert_eq!(question.options.len(), 1);
        assert!(question.multi_select);
        let json = serde_json::to_string(&question)?;
        assert_eq!(serde_json::from_str::<Question>(&json)?, question);
        Ok(())
    }

    #[test]
    fn asks_and_answers_round_trip() -> serde_json::Result<()> {
        let ask = AskKind::Plan {
            plan: "# Plan".to_owned(),
        };
        let json = serde_json::to_string(&ask)?;
        assert_eq!(json, r##"{"plan":{"plan":"# Plan"}}"##);
        assert_eq!(serde_json::from_str::<AskKind>(&json)?, ask);
        let answer = AskAnswer::Answers(vec![PickedAnswer {
            question: "Which?".to_owned(),
            labels: vec!["A".to_owned()],
        }]);
        let json = serde_json::to_string(&answer)?;
        assert_eq!(serde_json::from_str::<AskAnswer>(&json)?, answer);
        Ok(())
    }
}
