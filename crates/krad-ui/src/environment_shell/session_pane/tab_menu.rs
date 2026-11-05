use gpui_kit::{Context, PromptButton, PromptLevel, Window};
use krad_domain::{TabClosing, TerminalId};

use super::super::context_menu::{MenuAction, MenuEntry};
use super::super::file_tab::FileEditor;
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
const UNSAVED_LEAD: &str = "Your changes will be lost in";
const NAME_SEPARATOR: &str = ", ";
const DISCARD_LABEL: &str = "Close";
const CANCEL_LABEL: &str = "Cancel";

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
        let title = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == tab))
            .map_or_else(String::new, |node| node.title.clone());
        entries.extend(self.extension_menu_entries(
            krad_extensions::api::manifest::MenuTarget::Tab,
            None,
            &krad_extensions::api::protocol::CommandTarget::Tab {
                title,
                terminal: None,
            },
        ));
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
        if editing && !file {
            return live.data.session_chats();
        }
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
        let closed = closing.closed(&self.strip_of(tab), tab);
        let (dirty, clean): (Vec<TerminalId>, Vec<TerminalId>) = closed
            .into_iter()
            .partition(|closed| self.is_unsaved(*closed));
        for closed in clean {
            self.close_tab_asking(closed, window, cx);
        }
        match dirty.as_slice() {
            [] => {}
            [only] => self.request_close_tab(*only, window, cx),
            _ => self.ask_close_unsaved(dirty, window, cx),
        }
    }

    fn is_unsaved(&self, tab: TerminalId) -> bool {
        self.envs
            .get(&self.active)
            .and_then(|live| live.editors.get(&tab))
            .is_some_and(FileEditor::is_dirty)
    }

    fn ask_close_unsaved(
        &mut self,
        tabs: Vec<TerminalId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let names: Vec<String> = self.envs.get(&self.active).map_or_default(|live| {
            tabs.iter()
                .filter_map(|tab| live.data.nodes.iter().find(|node| node.id == *tab))
                .map(|node| node.title.clone())
                .collect()
        });
        let title = format!("Close {} files without saving?", tabs.len());
        let detail = format!("{UNSAVED_LEAD} {}.", names.join(NAME_SEPARATOR));
        let answer = window.prompt(
            PromptLevel::Warning,
            &title,
            Some(&detail),
            &[
                PromptButton::ok(DISCARD_LABEL),
                PromptButton::cancel(CANCEL_LABEL),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if matches!(answer.await, Ok(0)) {
                this.update(cx, |this, cx| {
                    for tab in tabs {
                        this.close_terminal(tab, cx);
                    }
                })
                .unwrap_or_default();
            }
        })
        .detach();
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
