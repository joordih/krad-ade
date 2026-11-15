use krad_domain::Settings;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum SettingsCommand {
    Save {
        settings: Box<Settings>,
    },
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Event, EventKind, Seq};
    use krad_domain::AppearanceMode;

    #[test]
    fn saving_settings_and_hearing_them_have_a_stable_wire_shape() -> serde_json::Result<()> {
        let settings = Settings {
            appearance: AppearanceMode::Dark,
            ..Settings::default()
        };
        let command = Command::Settings(SettingsCommand::Save {
            settings: Box::new(settings.clone()),
        });
        let json = serde_json::to_string(&command)?;
        assert!(json.starts_with(r#"{"area":"settings","verb":"save","settings":{"#));
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let newer: Command = serde_json::from_str(r#"{"area":"settings","verb":"reset"}"#)?;
        assert_eq!(newer, Command::Settings(SettingsCommand::Unknown));
        let event = Event {
            seq: Seq(3),
            kind: EventKind::SettingsChanged {
                settings: Box::new(settings),
            },
        };
        let json = serde_json::to_string(&event)?;
        assert!(json.starts_with(r#"{"seq":3,"kind":{"type":"settings_changed","settings":{"#));
        assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        let failed = Event {
            seq: Seq(4),
            kind: EventKind::SettingsNotSaved {
                error: "read-only".to_owned(),
            },
        };
        let json = serde_json::to_string(&failed)?;
        assert_eq!(
            json,
            r#"{"seq":4,"kind":{"type":"settings_not_saved","error":"read-only"}}"#
        );
        Ok(())
    }
}
