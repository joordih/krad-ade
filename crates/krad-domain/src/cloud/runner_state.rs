use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Linger {
    Enabled,
    Unavailable,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RunnerState {
    #[default]
    NotConnected,
    Connecting,
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
            Self::NotConnected | Self::Connecting | Self::Installing | Self::Failed { .. } => None,
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
}
