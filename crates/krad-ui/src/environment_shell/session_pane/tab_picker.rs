use gpui_kit::{AnyElement, ClickEvent, Context, IntoElement, Pixels, Point, Window};
use krad_domain::{AgentPreset, PaneId, TerminalId};

use super::super::context_menu::{MenuAction, MenuCreate, MenuEntry, MenuSearch};
use super::super::sessions::Launch;
use super::super::{EnvironmentShell, LiveEnv};
use crate::actions::ShowTabAt;
use crate::components::{Button, IconName};
use crate::keymap::{NEW_TERMINAL_HINT, TAB_SLOT_HINTS};

const BTN_TAB_PICKER: &str = "session-tab-picker";
const TAB_PICKER_TOOLTIP: &str = "Tabs";
const FILTER_PLACEHOLDER: &str = "Filter or create…";
const FILTER_FILES_PLACEHOLDER: &str = "Filter…";
const NEW_TAB_LABEL: &str = "New tab";

fn origin_of(event: &ClickEvent) -> (f32, f32) {
    let Point { x, y }: Point<Pixels> = event.position();
    (x.as_f32(), y.as_f32())
}

impl EnvironmentShell {
    fn picker_entries(
        &self,
        live: &LiveEnv,
        ids: &[TerminalId],
        shown: Option<TerminalId>,
    ) -> Vec<MenuEntry> {
        ids.iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let node = live.data.nodes.iter().find(|node| node.id == *id)?;
                let entry = MenuEntry::checked(
                    self.title_label(*id, &node.title),
                    shown == Some(*id),
                    MenuAction::ShowSessionTab(*id),
                );
                Some(match TAB_SLOT_HINTS.get(index) {
                    Some(hint) => entry.with_trailing(*hint),
                    None => entry,
                })
            })
            .collect()
    }

    fn picker_search(&self, pane: PaneId) -> MenuSearch {
        if self.editing() {
            return MenuSearch {
                placeholder: FILTER_FILES_PLACEHOLDER,
                create: None,
                footer: Vec::new(),
            };
        }
        MenuSearch {
            placeholder: FILTER_PLACEHOLDER,
            create: Some(MenuCreate::Tab(pane)),
            footer: vec![
                MenuEntry::Separator,
                MenuEntry::item(
                    NEW_TAB_LABEL,
                    Some(IconName::Add),
                    MenuAction::NewTabIn(pane),
                )
                .with_trailing(NEW_TERMINAL_HINT),
            ],
        }
    }

    pub(super) fn tab_picker_button(
        &self,
        live: &LiveEnv,
        pane: PaneId,
        ids: &[TerminalId],
        shown: Option<TerminalId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entries = std::rc::Rc::new(self.picker_entries(live, ids, shown));
        let search = std::rc::Rc::new(self.picker_search(pane));
        Button::new((BTN_TAB_PICKER, u64::from(pane.0)))
            .icon(IconName::Layers)
            .small()
            .tooltip(TAB_PICKER_TOOLTIP)
            .on_click(
                cx.listener(move |this: &mut Self, event: &ClickEvent, _, cx| {
                    this.open_searchable_menu(
                        origin_of(event),
                        entries.to_vec(),
                        (*search).clone(),
                        cx,
                    );
                }),
            )
            .into_any_element()
    }

    pub(in crate::environment_shell) fn new_tab_in(
        &mut self,
        pane: PaneId,
        cx: &mut Context<Self>,
    ) {
        self.pane_target = Some(pane);
        self.add_chat_tab(cx);
    }

    pub(in crate::environment_shell) fn create_named_terminal(
        &mut self,
        pane: PaneId,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        self.pane_target = Some(pane);
        self.spawn_terminal(name, Launch::Preset(AgentPreset::Shell), None, cx);
    }

    pub(in crate::environment_shell) fn on_show_tab_at(
        &mut self,
        action: &ShowTabAt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.envs.get(&self.active) else {
            return;
        };
        let Some(panes) = self.shown_panes(live) else {
            return;
        };
        let pane = live
            .focused
            .and_then(|focused| panes.pane_of(focused))
            .and_then(|pane| panes.pane(pane))
            .or_else(|| panes.panes.first());
        let Some(tab) = pane.and_then(|pane| pane.tabs.get(action.slot)).copied() else {
            return;
        };
        self.show_terminal_tab(tab, window, cx);
    }
}
