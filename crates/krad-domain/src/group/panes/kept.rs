use super::{PaneId, SessionPanes};
use crate::ids::TerminalId;

impl SessionPanes {
    #[must_use]
    pub fn keeping(&self, keep: impl Fn(TerminalId) -> bool) -> Option<Self> {
        let mut kept = self.clone();
        for pane in &mut kept.panes {
            pane.tabs.retain(|tab| keep(*tab));
            if pane.shown.is_none_or(|shown| !pane.tabs.contains(&shown)) {
                pane.shown = pane.tabs.last().copied();
            }
        }
        let emptied: Vec<PaneId> = kept
            .panes
            .iter()
            .filter(|pane| pane.tabs.is_empty())
            .map(|pane| pane.id)
            .collect();
        if emptied.len() == kept.panes.len() {
            return None;
        }
        for id in emptied {
            kept.remove_pane(id);
        }
        Some(kept)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{PaneNode, SnapZone};
    use super::*;

    const FILE_A: TerminalId = TerminalId(1);
    const CHAT: TerminalId = TerminalId(2);
    const FILE_B: TerminalId = TerminalId(3);

    fn is_file(id: TerminalId) -> bool {
        id != CHAT
    }

    #[test]
    fn a_pane_of_only_other_tabs_leaves_and_its_neighbour_takes_the_room() {
        let mut panes = SessionPanes::single(vec![FILE_A, CHAT]);
        assert!(panes.snap(CHAT, PaneId(0), SnapZone::Right).is_ok());
        let Some(kept) = panes.keeping(is_file) else {
            unreachable!("one pane still holds a file");
        };
        assert_eq!(kept.root, PaneNode::Pane(PaneId(0)));
        assert_eq!(kept.layout().len(), 1);
        assert_eq!(
            panes.panes.len(),
            2,
            "the session's own panes stay as they were"
        );
    }

    #[test]
    fn a_pane_showing_another_tab_shows_its_last_kept_one() {
        let mut panes = SessionPanes::single(vec![FILE_A, FILE_B, CHAT]);
        panes.show(CHAT);
        let shown = panes
            .keeping(is_file)
            .and_then(|kept| kept.pane(PaneId(0)).and_then(|pane| pane.shown));
        assert_eq!(shown, Some(FILE_B));
    }

    #[test]
    fn nothing_is_left_when_no_tab_is_kept() {
        let panes = SessionPanes::single(vec![CHAT]);
        assert!(panes.keeping(is_file).is_none());
    }
}
