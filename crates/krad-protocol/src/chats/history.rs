use krad_domain::{AgentPreset, ChatEntry, ResumeId, TerminalId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HistoryCursor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    #[serde(default)]
    pub next: u64,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HistoryAsk {
    pub chat: TerminalId,
    pub harness: AgentPreset,
    pub resume: ResumeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<PathBuf>,
    #[serde(default)]
    pub cursor: HistoryCursor,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "read", rename_all = "snake_case")]
pub enum HistoryPart {
    Appended {
        entries: Vec<ChatEntry>,
        cursor: HistoryCursor,
    },
    Whole {
        entries: Vec<ChatEntry>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Answer, ChatCommand, Command};
    use krad_domain::{ChatEvent, EnvironmentName};

    #[test]
    fn a_history_read_names_its_chat_and_answers_with_what_it_read() -> serde_json::Result<()> {
        let command = Command::Chat(ChatCommand::History {
            environment: EnvironmentName::new("main".to_owned()),
            ask: HistoryAsk {
                chat: TerminalId(4),
                harness: AgentPreset::Codex,
                resume: "thread-1"
                    .parse()
                    .map_err(|_| serde::de::Error::custom("resume"))?,
                root: None,
                cursor: HistoryCursor::default(),
            },
        });
        let json = serde_json::to_value(&command)?;
        assert_eq!(json["verb"], "history");
        assert_eq!(json["ask"]["cursor"]["next"], 0);
        assert_eq!(serde_json::from_value::<Command>(json)?, command);
        let answer = Answer::History(Box::new(HistoryPart::Appended {
            entries: vec![ChatEntry::Event(ChatEvent::Text("hi".to_owned()))],
            cursor: HistoryCursor {
                next: 42,
                ..HistoryCursor::default()
            },
        }));
        let json = serde_json::to_value(&answer)?;
        assert_eq!(json["kind"], "history");
        assert_eq!(json["read"], "appended");
        assert_eq!(serde_json::from_value::<Answer>(json)?, answer);
        Ok(())
    }
}
