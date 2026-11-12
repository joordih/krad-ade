use krad_domain::{AgentPreset, Host, ModelOffer};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelsOrigin {
    Listing,
    Chat,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum ModelsCommand {
    Keep {
        host: Host,
        harness: AgentPreset,
        models: Vec<ModelOffer>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ListedModels {
    pub host: Host,
    pub harness: AgentPreset,
    pub models: Vec<ModelOffer>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ModelsSnapshot {
    #[serde(default)]
    pub listed: Vec<ListedModels>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Event, EventKind, Seq};
    use krad_domain::{ContextSize, Effort, ModelSource};

    fn offer() -> ModelOffer {
        ModelOffer {
            value: "opus[1m]".to_owned(),
            name: "Opus".to_owned(),
            efforts: vec![Effort::High],
            source: Some(ModelSource::Plan {
                name: "Max".to_owned(),
                order: 1,
            }),
            context: Some(ContextSize::OneMillion),
            ..ModelOffer::default()
        }
    }

    #[test]
    fn a_kept_model_list_has_a_stable_wire_shape() -> serde_json::Result<()> {
        let command = Command::Models(ModelsCommand::Keep {
            host: Host::Local,
            harness: AgentPreset::Codex,
            models: Vec::new(),
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"models","verb":"keep","host":{"kind":"local"},"harness":"Codex","models":[]}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let newer: Command = serde_json::from_str(r#"{"area":"models","verb":"forget"}"#)?;
        assert_eq!(newer, Command::Models(ModelsCommand::Unknown));
        Ok(())
    }

    #[test]
    fn model_events_round_trip_with_their_offers() -> serde_json::Result<()> {
        let events = [
            EventKind::ModelsListed {
                listed: ListedModels {
                    host: Host::Local,
                    harness: AgentPreset::ClaudeCode,
                    models: vec![offer()],
                },
                origin: ModelsOrigin::Listing,
            },
            EventKind::ModelListFailed {
                host: Host::Local,
                harness: AgentPreset::Codex,
                error: "no app server".to_owned(),
            },
        ];
        for kind in events {
            let event = Event { seq: Seq(1), kind };
            let json = serde_json::to_string(&event)?;
            assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        }
        Ok(())
    }
}
