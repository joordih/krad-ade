use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::view::EnvironmentView;

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
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden_views: BTreeSet<EnvironmentView>,
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
    pub fn shows_view(&self, view: EnvironmentView) -> bool {
        !view.is_optional() || !self.hidden_views.contains(&view)
    }

    pub fn toggle_view(&mut self, view: EnvironmentView) {
        if !view.is_optional() {
            return;
        }
        if !self.hidden_views.remove(&view) {
            self.hidden_views.insert(view);
        }
    }

    #[must_use]
    pub fn reachable_view(&self, view: EnvironmentView) -> EnvironmentView {
        if self.shows_view(view) {
            view
        } else {
            EnvironmentView::Sessions
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
            hidden_views: BTreeSet::new(),
        };
        let text = serde_json::to_string(&layout)?;
        assert_eq!(text, r#"{"hidden":["agent_counts"],"density":"compact"}"#);
        assert_eq!(serde_json::from_str::<SidebarLayout>(&text)?, layout);
        Ok(())
    }

    #[test]
    fn hidden_views_round_trip_and_older_settings_show_every_view() -> serde_json::Result<()> {
        let mut layout = SidebarLayout::default();
        layout.toggle_view(EnvironmentView::Kraddies);
        layout.toggle_view(EnvironmentView::Review);
        let text = serde_json::to_string(&layout)?;
        assert_eq!(
            text,
            r#"{"density":"normal","hidden_views":["Kraddies","Review"]}"#
        );
        assert_eq!(serde_json::from_str::<SidebarLayout>(&text)?, layout);
        let older: SidebarLayout = serde_json::from_str(r#"{"density":"normal"}"#)?;
        assert!(EnvironmentView::ALL
            .iter()
            .all(|view| older.shows_view(*view)));
        Ok(())
    }

    #[test]
    fn sessions_always_stays_while_optional_views_switch_off() {
        let mut layout = SidebarLayout::default();
        for view in EnvironmentView::ALL {
            layout.toggle_view(view);
        }
        assert!(layout.shows_view(EnvironmentView::Sessions));
        assert!(!layout.shows_view(EnvironmentView::Notes));
        assert!(!layout.shows_view(EnvironmentView::Automations));
        assert!(!layout.shows_view(EnvironmentView::Kraddies));
        assert!(!layout.shows_view(EnvironmentView::Review));
        layout.toggle_view(EnvironmentView::Automations);
        assert!(layout.shows_view(EnvironmentView::Automations));
    }

    #[test]
    fn a_hidden_active_view_falls_back_to_sessions() {
        let mut layout = SidebarLayout::default();
        assert_eq!(
            layout.reachable_view(EnvironmentView::Kraddies),
            EnvironmentView::Kraddies
        );
        layout.toggle_view(EnvironmentView::Kraddies);
        assert_eq!(
            layout.reachable_view(EnvironmentView::Kraddies),
            EnvironmentView::Sessions
        );
        layout.toggle_view(EnvironmentView::Notes);
        assert_eq!(
            layout.reachable_view(EnvironmentView::Notes),
            EnvironmentView::Sessions
        );
    }
}
