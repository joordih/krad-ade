use krad_domain::{GatewayKey, GatewayModel, GatewayUrl, LoginPlace};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum GatewayCommand {
    Check {
        url: Option<GatewayUrl>,
        key: Option<GatewayKey>,
    },
    ForgetKey {
        url: GatewayUrl,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GatewayState {
    #[default]
    Unset,
    Checking,
    Ready {
        place: LoginPlace,
        models: Vec<GatewayModel>,
    },
    Failed {
        has_key: bool,
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Event, EventKind, Seq};

    #[test]
    fn a_gateway_check_and_its_outcome_have_a_stable_wire_shape() -> serde_json::Result<()> {
        let command = Command::Gateway(GatewayCommand::ForgetKey {
            url: "http://127.0.0.1:8317"
                .parse()
                .unwrap_or_else(|_| unreachable!("a loopback address")),
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"gateway","verb":"forget_key","url":"http://127.0.0.1:8317"}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let event = Event {
            seq: Seq(2),
            kind: EventKind::GatewayChanged {
                state: GatewayState::Ready {
                    place: LoginPlace::Keyring,
                    models: Vec::new(),
                },
            },
        };
        let json = serde_json::to_string(&event)?;
        assert_eq!(
            json,
            r#"{"seq":2,"kind":{"type":"gateway_changed","state":{"state":"ready","place":"keyring","models":[]}}}"#
        );
        assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        Ok(())
    }
}
