use crate::ids::TerminalId;
use serde::{Deserialize, Serialize};

pub const SIDE_PANEL_DEFAULT_W: f32 = 300.0;
pub const SIDE_PANEL_MIN_W: f32 = 220.0;
pub const SIDE_PANEL_MAX_W: f32 = 640.0;
pub const SIDE_PANEL_DEFAULT_SPLIT: f32 = 0.6;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemTerminal {
    pub id: TerminalId,
    pub title: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SidePanelSection {
    #[default]
    Explorer,
    Changes,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangesLayout {
    #[default]
    Tree,
    List,
}

impl ChangesLayout {
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Tree => Self::List,
            Self::List => Self::Tree,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SidePanelLayout {
    #[serde(default)]
    pub open: bool,
    #[serde(default = "default_width")]
    pub width: f32,
    #[serde(default = "default_split")]
    pub split: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terminals: Vec<SystemTerminal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shown: Option<TerminalId>,
    #[serde(default)]
    pub terminals_open: bool,
    #[serde(default)]
    pub section: SidePanelSection,
    #[serde(default)]
    pub changes_layout: ChangesLayout,
}

const fn default_width() -> f32 {
    SIDE_PANEL_DEFAULT_W
}

const fn default_split() -> f32 {
    SIDE_PANEL_DEFAULT_SPLIT
}

impl Default for SidePanelLayout {
    fn default() -> Self {
        Self {
            open: false,
            width: SIDE_PANEL_DEFAULT_W,
            split: SIDE_PANEL_DEFAULT_SPLIT,
            terminals: Vec::new(),
            shown: None,
            terminals_open: false,
            section: SidePanelSection::default(),
            changes_layout: ChangesLayout::default(),
        }
    }
}

impl SidePanelLayout {
    pub fn set_width(&mut self, width: f32) {
        self.width = width.clamp(SIDE_PANEL_MIN_W, SIDE_PANEL_MAX_W);
    }

    #[must_use]
    pub fn holds(&self, id: TerminalId) -> bool {
        self.terminals.iter().any(|terminal| terminal.id == id)
    }

    #[must_use]
    pub fn shown_terminal(&self) -> Option<TerminalId> {
        self.shown
            .filter(|id| self.holds(*id))
            .or_else(|| self.terminals.first().map(|terminal| terminal.id))
    }

    pub fn add_terminal(&mut self, id: TerminalId, title: String) {
        self.terminals.push(SystemTerminal { id, title });
        self.shown = Some(id);
    }

    pub fn remove_terminal(&mut self, id: TerminalId) {
        let Some(index) = self.terminals.iter().position(|terminal| terminal.id == id) else {
            return;
        };
        self.terminals.remove(index);
        if self.shown == Some(id) {
            self.shown = self
                .terminals
                .get(index)
                .or_else(|| self.terminals.last())
                .map(|terminal| terminal.id);
        }
        if self.terminals.is_empty() {
            self.terminals_open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_the_shown_terminal_shows_its_neighbour() {
        let mut panel = SidePanelLayout::default();
        for id in 1..=3 {
            panel.add_terminal(TerminalId(id), format!("zsh · {id}"));
        }
        assert_eq!(panel.shown_terminal(), Some(TerminalId(3)));
        panel.shown = Some(TerminalId(2));
        panel.remove_terminal(TerminalId(2));
        assert_eq!(panel.shown_terminal(), Some(TerminalId(3)));
        panel.remove_terminal(TerminalId(3));
        assert_eq!(panel.shown_terminal(), Some(TerminalId(1)));
        panel.terminals_open = true;
        panel.remove_terminal(TerminalId(1));
        assert_eq!(panel.shown_terminal(), None);
        assert!(!panel.terminals_open);
    }

    #[test]
    fn layouts_saved_before_the_side_panel_load_with_its_defaults() {
        let panel = serde_json::from_str::<SidePanelLayout>("{}");
        assert!(panel.is_ok_and(|panel| panel == SidePanelLayout::default()));
        let saved = serde_json::to_string(&SidePanelLayout::default()).unwrap_or_default();
        assert!(!saved.contains("\"terminals\""), "{saved}");
    }
}
