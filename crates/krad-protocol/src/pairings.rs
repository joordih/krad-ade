use krad_domain::{
    DeviceName, InstallId, PairedDevice, PairingCode, PairingPermissions, PairingToken, Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum PairingCommand {
    CreatePairingCode {
        permissions: PairingPermissions,
    },
    RedeemPairingCode {
        code: PairingCode,
        install: InstallId,
        device: DeviceName,
    },
    ListPairings,
    RevokePairing {
        install: InstallId,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PairingAnswer {
    Code {
        code: PairingCode,
        expires: Timestamp,
    },
    Redeemed {
        token: PairingToken,
        permissions: PairingPermissions,
    },
    Pairings {
        devices: Vec<PairedDevice>,
    },
    Revoked,
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_redeemed_token_reaches_the_wire_but_never_a_log() -> Result<(), Box<dyn std::error::Error>>
    {
        let token: PairingToken = "00112233445566778899aabbccddeeff".parse()?;
        let answer = PairingAnswer::Redeemed {
            token: token.clone(),
            permissions: PairingPermissions::all(),
        };
        let json = serde_json::to_string(&answer)?;
        assert!(json.contains(token.expose()));
        assert!(!format!("{answer:?}").contains(token.expose()));
        assert_eq!(serde_json::from_str::<PairingAnswer>(&json)?, answer);
        let newer: PairingCommand = serde_json::from_str(r#"{"verb":"rename_pairing"}"#)?;
        assert_eq!(newer, PairingCommand::Unknown);
        Ok(())
    }
}
