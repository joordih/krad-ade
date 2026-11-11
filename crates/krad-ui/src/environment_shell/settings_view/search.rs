use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    div, AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Styled, Subscription, Window,
};

use super::navigation::{next_pane, search, PaneMatch};
use super::EnvironmentShell;
use crate::components::{icon, IconName};
use crate::theme::{palette, ICON_SM};

const SEARCH_PLACEHOLDER: &str = "Search settings";
const SEARCH_ID: &str = "settings-search";
const UP_KEY: &str = "up";
const DOWN_KEY: &str = "down";
const ENTER_KEY: &str = "enter";
const ESCAPE_KEY: &str = "escape";
const STEP_UP: isize = -1;
const STEP_DOWN: isize = 1;

pub(in crate::environment_shell) struct SearchUi {
    field: Entity<InputState>,
    query: String,
    _edit: Subscription,
}

impl SearchUi {
    pub(super) fn matches(&self) -> Vec<PaneMatch> {
        search(&self.query)
    }

    pub(super) fn is_searching(&self) -> bool {
        !self.query.trim().is_empty()
    }
}

impl EnvironmentShell {
    pub(super) fn settings_search_ui(window: &mut Window, cx: &mut Context<Self>) -> SearchUi {
        let field = cx.new(|cx| InputState::new(window, cx).placeholder(SEARCH_PLACEHOLDER));
        let edit = cx.subscribe(&field, |this: &mut Self, field, event: &InputEvent, cx| {
            if !matches!(event, InputEvent::Change) {
                return;
            }
            let query = field.read(cx).value().to_string();
            this.search_settings(query, cx);
        });
        SearchUi {
            field,
            query: String::new(),
            _edit: edit,
        }
    }

    fn search_settings(&mut self, query: String, cx: &mut Context<Self>) {
        let Some(ui) = self.settings_ui.as_mut() else {
            return;
        };
        ui.search.query = query;
        let matches = ui.search.matches();
        let hidden = matches.iter().all(|hit| hit.pane != ui.pane);
        if let Some(first) = matches.first().filter(|_| hidden) {
            self.select_pane(first.pane, cx);
        }
        cx.notify();
    }

    fn step_searched_pane(&mut self, step: isize, cx: &mut Context<Self>) {
        let Some(ui) = self.settings_ui.as_ref() else {
            return;
        };
        let listed: Vec<_> = ui
            .search
            .matches()
            .into_iter()
            .map(|hit| hit.pane)
            .collect();
        let next = next_pane(ui.pane, &listed, step);
        if next != ui.pane {
            self.select_pane(next, cx);
        }
    }

    fn clear_settings_search(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(ui) = self.settings_ui.as_mut() else {
            return false;
        };
        if !ui.search.is_searching() {
            return false;
        }
        ui.search.query.clear();
        let field = ui.search.field.clone();
        field.update(cx, |field, cx| field.set_value("", window, cx));
        cx.notify();
        true
    }

    fn on_search_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            UP_KEY => self.step_searched_pane(STEP_UP, cx),
            DOWN_KEY => self.step_searched_pane(STEP_DOWN, cx),
            ENTER_KEY => {
                let first = self
                    .settings_ui
                    .as_ref()
                    .and_then(|ui| ui.search.matches().first().map(|hit| hit.pane));
                if let Some(first) = first {
                    self.select_pane(first, cx);
                }
            }
            ESCAPE_KEY if self.clear_settings_search(window, cx) => {}
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(super) fn settings_search_field(ui: &SearchUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .id(SEARCH_ID)
            .w_full()
            .capture_key_down(cx.listener(Self::on_search_key))
            .child(
                Input::new(&ui.field)
                    .prefix(icon(IconName::Search, ICON_SM, pal.text_muted))
                    .cleanable(true),
            )
            .into_any_element()
    }
}
