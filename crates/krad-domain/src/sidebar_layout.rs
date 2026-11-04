use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SidebarPart {
    Views,
    Branches,
    AgentCounts,
}

impl SidebarPart {
    pub const ALL: [Self; 3] = [Self::Views, Self::Branches, Self::AgentCounts];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Views => "Views",
            Self::Branches => "Branches",
            Self::AgentCounts => "Agent counts",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarDensity {
    #[default]
    Normal,
    Compact,
}

impl SidebarDensity {
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Normal => Self::Compact,
            Self::Compact => Self::Normal,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarLayout {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden: BTreeSet<SidebarPart>,
    #[serde(default)]
    pub density: SidebarDensity,
}

impl SidebarLayout {
    #[must_use]
    pub fn shows(&self, part: SidebarPart) -> bool {
        !self.hidden.contains(&part)
    }

    pub fn toggle(&mut self, part: SidebarPart) {
        if !self.hidden.remove(&part) {
            self.hidden.insert(part);
        }
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_shows_until_turned_off_and_back_on() {
        let mut layout = SidebarLayout::default();
        assert!(SidebarPart::ALL.iter().all(|part| layout.shows(*part)));
        layout.toggle(SidebarPart::Branches);
        assert!(!layout.shows(SidebarPart::Branches));
        layout.toggle(SidebarPart::Branches);
        assert!(layout.is_default());
    }

    #[test]
    fn a_compact_sidebar_round_trips_through_settings() -> serde_json::Result<()> {
        let layout = SidebarLayout {
            hidden: BTreeSet::from([SidebarPart::AgentCounts]),
            density: SidebarDensity::Compact,
        };
        let text = serde_json::to_string(&layout)?;
        assert_eq!(text, r#"{"hidden":["agent_counts"],"density":"compact"}"#);
        assert_eq!(serde_json::from_str::<SidebarLayout>(&text)?, layout);
        Ok(())
    }
}
