use serde::{Deserialize, Serialize};

use super::CloudRunnerId;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum RuntimeId {
    #[default]
    Local,
    Runner(CloudRunnerId),
}

impl RuntimeId {
    #[must_use]
    pub const fn runner(self) -> Option<CloudRunnerId> {
        match self {
            Self::Local => None,
            Self::Runner(runner) => Some(runner),
        }
    }
}

impl From<CloudRunnerId> for RuntimeId {
    fn from(runner: CloudRunnerId) -> Self {
        Self::Runner(runner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runner_s_runtime_names_its_runner_and_the_local_one_none() {
        let runner = CloudRunnerId(3);
        assert_eq!(RuntimeId::from(runner).runner(), Some(runner));
        assert_eq!(RuntimeId::Local.runner(), None);
        assert!(RuntimeId::Local < RuntimeId::Runner(CloudRunnerId(0)));
    }
}
