use gpui_kit::{Context, Window};
use krad_domain::{TabClosing, TerminalId};

use super::super::context_menu::{MenuAction, MenuEntry};
use super::super::EnvironmentShell;
use crate::actions::{CloseAllTabs, CloseTab};
use crate::components::IconName;
use crate::keymap::{CLOSE_ALL_TABS_HINT, CLOSE_TAB_HINT};

const PIN_LABEL: &str = "Pin Tab";
const RENAME_LABEL: &str = "Rename";
const CLOSE_LABEL: &str = "Close Tab";
const CLOSE_OTHERS_LABEL: &str = "Close Other Tabs";
const CLOSE_RIGHT_LABEL: &str = "Close Tabs to the Right";
const CLOSE_LEFT_LABEL: &str = "Close Tabs to the Left";
const CLOSE_ALL_LABEL: &str = "Close All Tabs";

fn closing_item(label: &str, tab: TerminalId, closing: TabClosing) -> MenuEntry {
    MenuEntry::item(label, None, MenuAction::CloseTabs(tab, closing))
}

impl EnvironmentShell {
    pub(super) fn tab_menu(&self, tab: TerminalId) -> Vec<MenuEntry> {
        let preview = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == tab))
            .and_then(|node| node.file.as_ref())
            .is_some_and(|file| !file.pinned);
        let mut entries = Vec::new();
        if preview {
            entries.push(MenuEntry::item(
                PIN_LABEL,
                Some(IconName::Tick),
                MenuAction::PinTab(tab),
            ));
        }
        entries.extend([
            MenuEntry::item(RENAME_LABEL, None, MenuAction::RenameTerminal(tab)),
            MenuEntry::Separator,
            closing_item(CLOSE_LABEL, tab, TabClosing::This).with_trailing(CLOSE_TAB_HINT),
            MenuEntry::Separator,
            closing_item(CLOSE_OTHERS_LABEL, tab, TabClosing::Others),
            closing_item(CLOSE_RIGHT_LABEL, tab, TabClosing::ToTheRight),
            closing_item(CLOSE_LEFT_LABEL, tab, TabClosing::ToTheLeft),
            closing_item(CLOSE_ALL_LABEL, tab, TabClosing::All).with_trailing(CLOSE_ALL_TABS_HINT),
        ]);
        entries
    }

    fn strip_of(&self, tab: TerminalId) -> Vec<TerminalId> {
        let Some(live) = self.envs.get(&self.active) else {
            return Vec::new();
        };
        let Some(panes) = live.data.current_session().map(|session| &session.panes) else {
            return Vec::new();
        };
        let Some(pane) = panes.pane_of(tab).and_then(|pane| panes.pane(pane)) else {
            return Vec::new();
        };
        let file = live.data.is_file_tab(tab);
        let editing = self.editing();
        pane.tabs
            .iter()
            .copied()
            .filter(|other| !editing || live.data.is_file_tab(*other) == file)
            .collect()
    }

    pub(in crate::environment_shell) fn close_tabs(
        &mut self,
        tab: TerminalId,
        closing: TabClosing,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for closed in closing.closed(&self.strip_of(tab), tab) {
            self.request_close_tab(closed, window, cx);
        }
    }

    fn current_tab(&self) -> Option<TerminalId> {
        let live = self.envs.get(&self.active)?;
        if self.editing() {
            return live.shown_file().map(|(tab, _)| tab);
        }
        live.shown_tab()
    }

    pub(in crate::environment_shell) fn on_close_tab(
        &mut self,
        _: &CloseTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.current_tab() {
            self.close_tabs(tab, TabClosing::This, window, cx);
        }
    }

    pub(in crate::environment_shell) fn on_close_all_tabs(
        &mut self,
        _: &CloseAllTabs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.current_tab() {
            self.close_tabs(tab, TabClosing::All, window, cx);
        }
    }
}
