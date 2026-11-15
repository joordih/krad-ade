use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Linger {
    Enabled,
    Unavailable,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RunnerState {
    #[default]
    NotConnected,
    Connecting,
    SigningIn,
    Installing,
    Ready {
        linger: Linger,
    },
    UpdatePending {
        linger: Linger,
    },
    Failed {
        reason: String,
    },
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

impl RunnerState {
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. } | Self::UpdatePending { .. })
    }

    #[must_use]
    pub const fn is_busy(&self) -> bool {
        matches!(self, Self::Connecting | Self::Installing)
    }

    #[must_use]
    pub const fn linger(&self) -> Option<Linger> {
        match self {
            Self::Ready { linger } | Self::UpdatePending { linger } => Some(*linger),
            Self::NotConnected
            | Self::Connecting
            | Self::SigningIn
            | Self::Installing
            | Self::Failed { .. }
            | Self::Unknown => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_runner_on_its_way_is_busy_and_only_a_ready_one_has_a_linger() {
        assert!(RunnerState::Connecting.is_busy());
        assert!(RunnerState::Installing.is_busy());
        assert!(!RunnerState::NotConnected.is_busy());
        assert!(!RunnerState::SigningIn.is_busy());
        let failed = RunnerState::Failed {
            reason: "no route".to_owned(),
        };
        assert!(!failed.is_ready());
        assert_eq!(failed.linger(), None);
        let pending = RunnerState::UpdatePending {
            linger: Linger::Unavailable,
        };
        assert!(pending.is_ready());
        assert_eq!(pending.linger(), Some(Linger::Unavailable));
    }

    #[test]
    fn a_signing_in_runner_round_trips_and_a_newer_state_reads_as_unknown() -> serde_json::Result<()>
    {
        let json = serde_json::to_string(&RunnerState::SigningIn)?;
        assert_eq!(json, r#"{"state":"signing_in"}"#);
        assert_eq!(
            serde_json::from_str::<RunnerState>(&json)?,
            RunnerState::SigningIn
        );
        assert_eq!(
            serde_json::from_str::<RunnerState>(r#"{"state":"migrating"}"#)?,
            RunnerState::Unknown
        );
        Ok(())
    }
}
