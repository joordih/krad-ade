use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolchainActivation {
    #[default]
    Off,
    On,
}

impl ToolchainActivation {
    #[must_use]
    pub const fn is_off(&self) -> bool {
        matches!(self, Self::Off)
    }

    #[must_use]
    pub const fn is_on(self) -> bool {
        matches!(self, Self::On)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupActivation {
    #[default]
    Inherit,
    On,
    Off,
}

impl GroupActivation {
    pub const ALL: [Self; 3] = [Self::Inherit, Self::On, Self::Off];

    #[must_use]
    pub const fn is_inherit(&self) -> bool {
        matches!(self, Self::Inherit)
    }

    #[must_use]
    pub const fn resolve(self, global: ToolchainActivation) -> ToolchainActivation {
        match self {
            Self::Inherit => global,
            Self::On => ToolchainActivation::On,
            Self::Off => ToolchainActivation::Off,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_inherits_the_global_switch_unless_it_overrides_it() {
        assert_eq!(
            GroupActivation::Inherit.resolve(ToolchainActivation::On),
            ToolchainActivation::On
        );
        assert_eq!(
            GroupActivation::Off.resolve(ToolchainActivation::On),
            ToolchainActivation::Off
        );
        assert_eq!(
            GroupActivation::On.resolve(ToolchainActivation::Off),
            ToolchainActivation::On
        );
        assert!(ToolchainActivation::default().is_off());
        assert!(GroupActivation::default().is_inherit());
    }
}
