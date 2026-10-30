use serde::{Deserialize, Serialize};

use crate::group::FileOpening;
use crate::ids::GroupId;

const PATH_SEPARATOR: char = '/';

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteMode {
    #[default]
    Source,
    Reading,
    Split,
}

impl NoteMode {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Source => Self::Reading,
            Self::Reading => Self::Split,
            Self::Split => Self::Source,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteTab {
    pub path: String,
    pub pinned: bool,
    #[serde(default)]
    pub mode: NoteMode,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteTabs {
    pub group: Option<GroupId>,
    pub tabs: Vec<NoteTab>,
    pub active: Option<usize>,
}

fn is_under(path: &str, folder: &str) -> bool {
    path == folder
        || path
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with(PATH_SEPARATOR))
}

impl NoteTabs {
    #[must_use]
    pub fn for_group(group: GroupId) -> Self {
        Self {
            group: Some(group),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn active_tab(&self) -> Option<&NoteTab> {
        self.active.and_then(|index| self.tabs.get(index))
    }

    pub fn open(&mut self, path: &str, opening: FileOpening) {
        let pinned = opening == FileOpening::Pinned;
        if let Some(index) = self.tabs.iter().position(|tab| tab.path == path) {
            self.tabs[index].pinned |= pinned;
            self.active = Some(index);
            return;
        }
        let tab = NoteTab {
            path: path.to_owned(),
            pinned,
            mode: NoteMode::default(),
        };
        if let (false, Some(index)) = (pinned, self.tabs.iter().position(|tab| !tab.pinned)) {
            self.tabs[index] = tab;
            self.active = Some(index);
            return;
        }
        let at = self
            .active
            .map_or(self.tabs.len(), |active| active + 1)
            .min(self.tabs.len());
        self.tabs.insert(at, tab);
        self.active = Some(at);
    }

    pub fn pin(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.pinned = true;
        }
    }

    pub fn show(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = Some(index);
        }
    }

    pub fn close(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        self.active = match self.active {
            _ if self.tabs.is_empty() => None,
            Some(active) if active > index => Some(active - 1),
            Some(active) if active == index => Some(index.min(self.tabs.len() - 1)),
            other => other,
        };
    }

    pub fn set_mode(&mut self, index: usize, mode: NoteMode) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.mode = mode;
        }
    }

    pub fn rename(&mut self, from: &str, to: &str) {
        for tab in &mut self.tabs {
            if is_under(&tab.path, from) {
                tab.path = format!("{to}{}", &tab.path[from.len()..]);
            }
        }
    }

    pub fn remove(&mut self, path: &str) {
        while let Some(index) = self.tabs.iter().position(|tab| is_under(&tab.path, path)) {
            self.close(index);
        }
    }

    pub fn drop_preview(&mut self) {
        if let Some(index) = self.tabs.iter().position(|tab| !tab.pinned) {
            self.close(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(tabs: &NoteTabs) -> Vec<(&str, bool)> {
        tabs.tabs
            .iter()
            .map(|tab| (tab.path.as_str(), tab.pinned))
            .collect()
    }

    #[test]
    fn one_click_replaces_the_preview_tab_and_a_double_click_pins() {
        let mut tabs = NoteTabs::default();
        tabs.open("a.md", FileOpening::Preview);
        tabs.open("b.md", FileOpening::Preview);
        assert_eq!(paths(&tabs), [("b.md", false)]);
        tabs.open("b.md", FileOpening::Pinned);
        tabs.open("c.md", FileOpening::Preview);
        assert_eq!(paths(&tabs), [("b.md", true), ("c.md", false)]);
        tabs.open("b.md", FileOpening::Preview);
        assert_eq!(tabs.active, Some(0), "an open note shows its tab");
        tabs.open("d.md", FileOpening::Pinned);
        assert_eq!(
            paths(&tabs),
            [("b.md", true), ("d.md", true), ("c.md", false)]
        );
    }

    #[test]
    fn closing_shows_the_neighbour_and_preview_tabs_drop() {
        let mut tabs = NoteTabs::default();
        for path in ["a.md", "b.md", "c.md"] {
            tabs.open(path, FileOpening::Pinned);
        }
        tabs.show(1);
        tabs.close(1);
        assert_eq!(tabs.active_tab().map(|tab| tab.path.as_str()), Some("c.md"));
        tabs.close(1);
        assert_eq!(tabs.active_tab().map(|tab| tab.path.as_str()), Some("a.md"));
        tabs.open("p.md", FileOpening::Preview);
        tabs.drop_preview();
        assert_eq!(paths(&tabs), [("a.md", true)]);
        tabs.close(0);
        assert_eq!(tabs.active, None);
    }

    #[test]
    fn renames_and_removals_follow_folders() {
        let mut tabs = NoteTabs::default();
        tabs.open("Ideas/a.md", FileOpening::Pinned);
        tabs.open("Ideas2.md", FileOpening::Pinned);
        tabs.rename("Ideas", "Archive/Ideas");
        assert_eq!(
            paths(&tabs),
            [("Archive/Ideas/a.md", true), ("Ideas2.md", true)]
        );
        tabs.remove("Archive");
        assert_eq!(paths(&tabs), [("Ideas2.md", true)]);
    }

    #[test]
    fn the_note_mode_cycles_from_source_to_reading_to_split_and_back() {
        assert_eq!(NoteMode::Source.next(), NoteMode::Reading);
        assert_eq!(NoteMode::Reading.next(), NoteMode::Split);
        assert_eq!(NoteMode::Split.next(), NoteMode::Source);
    }

    #[test]
    fn tabs_saved_before_modes_load_in_source_mode() {
        let tab: Result<NoteTab, _> = serde_json::from_str(r#"{"path":"a.md","pinned":true}"#);
        assert!(tab.is_ok_and(|tab| tab.mode == NoteMode::Source));
    }
}
