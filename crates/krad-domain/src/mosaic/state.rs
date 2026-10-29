use serde::{Deserialize, Serialize};

use super::{normalize, MosaicTab, MAIN_RATIO_MAX, MAIN_RATIO_MIN};
use crate::ids::{TabId, TerminalId};

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MosaicState {
    pub tabs: Vec<MosaicTab>,
    pub active: TabId,
    pub next_tab: u64,
}

impl MosaicState {
    #[must_use]
    pub fn with_terminals(terminals: &[TerminalId]) -> Self {
        let mut state = Self::default();
        state.reconcile(terminals);
        state
    }

    pub fn reconcile(&mut self, terminals: &[TerminalId]) {
        let mut seen = Vec::new();
        for tab in &mut self.tabs {
            tab.tiles.retain(|tile| {
                let keep = terminals.contains(&tile.id) && !seen.contains(&tile.id);
                seen.push(tile.id);
                keep
            });
            for tile in &mut tab.tiles {
                if !tile.share.is_finite() || tile.share <= 0.0 {
                    tile.share = 1.0;
                }
            }
            normalize(&mut tab.tiles);
            tab.main_ratio = tab.main_ratio.clamp(MAIN_RATIO_MIN, MAIN_RATIO_MAX);
        }
        if self.tabs.is_empty() {
            self.new_tab();
        }
        if !self.tabs.iter().any(|tab| tab.id == self.active) {
            self.active = self.tabs[0].id;
        }
        let used = self
            .tabs
            .iter()
            .map(|tab| tab.id.as_u64())
            .max()
            .unwrap_or(0);
        self.next_tab = self.next_tab.max(used + 1);
        for id in terminals {
            if self.tab_of(*id).is_none() {
                self.add_terminal(*id);
            }
        }
    }

    pub fn new_tab(&mut self) -> TabId {
        let id = TabId(self.next_tab);
        self.next_tab += 1;
        let name = (self.tabs.len() + 1).to_string();
        self.tabs.push(MosaicTab::new(id, name));
        self.active = id;
        id
    }

    pub fn select(&mut self, id: TabId) -> bool {
        let exists = self.tabs.iter().any(|tab| tab.id == id);
        if exists {
            self.active = id;
        }
        exists
    }

    #[must_use]
    pub fn active_tab(&self) -> Option<&MosaicTab> {
        self.tabs.iter().find(|tab| tab.id == self.active)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut MosaicTab> {
        let active = self.active;
        self.tabs.iter_mut().find(|tab| tab.id == active)
    }

    #[must_use]
    pub fn tab_of(&self, id: TerminalId) -> Option<TabId> {
        self.tabs
            .iter()
            .find(|tab| tab.contains(id))
            .map(|tab| tab.id)
    }

    pub fn add_terminal(&mut self, id: TerminalId) {
        if let Some(tab) = self.active_tab_mut() {
            tab.insert(id);
        }
    }

    pub fn add_terminal_after(&mut self, id: TerminalId, anchor: TerminalId) {
        for tab in &mut self.tabs {
            if let Some(index) = tab.tiles.iter().position(|tile| tile.id == anchor) {
                tab.insert_at(index + 1, id);
                return;
            }
        }
        self.add_terminal(id);
    }

    pub fn remove_terminal(&mut self, id: TerminalId) {
        for tab in &mut self.tabs {
            if tab.remove(id) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Tile;
    use super::*;
    use crate::test_support::assert_close;

    fn ids(count: u64) -> Vec<TerminalId> {
        (1..=count).map(TerminalId).collect()
    }

    #[test]
    fn new_terminals_join_the_active_tab_with_a_fair_share() {
        let mut state = MosaicState::with_terminals(&ids(2));
        let second = state.new_tab();
        state.add_terminal(TerminalId(3));
        state.add_terminal(TerminalId(4));
        assert_eq!(state.tab_of(TerminalId(3)), Some(second));
        assert_eq!(state.active, second, "the new tab is active");
        let Some(tab) = state.active_tab() else {
            return;
        };
        for tile in &tab.tiles {
            assert_close(tile.share, 0.5);
        }
    }

    #[test]
    fn splitting_places_the_new_terminal_next_to_its_anchor() {
        let mut state = MosaicState::with_terminals(&ids(3));
        state.add_terminal_after(TerminalId(9), TerminalId(1));
        let order: Vec<TerminalId> = state
            .active_tab()
            .map_or_default(|tab| tab.tiles.iter().map(|tile| tile.id).collect());
        assert_eq!(
            order,
            vec![TerminalId(1), TerminalId(9), TerminalId(2), TerminalId(3)]
        );
    }

    #[test]
    fn reconcile_keeps_each_terminal_in_exactly_one_tab() {
        let mut state = MosaicState::with_terminals(&ids(3));
        let other = state.new_tab();
        if let Some(tab) = state.tabs.iter_mut().find(|tab| tab.id == other) {
            tab.tiles.push(Tile {
                id: TerminalId(1),
                share: 1.0,
            });
            tab.tiles.push(Tile {
                id: TerminalId(99),
                share: 1.0,
            });
        }
        state.reconcile(&ids(4));
        for id in ids(4) {
            let holders = state.tabs.iter().filter(|tab| tab.contains(id)).count();
            assert_eq!(holders, 1, "{id:?} lives in exactly one tab");
        }
        assert_eq!(state.tab_of(TerminalId(99)), None);
    }
}
