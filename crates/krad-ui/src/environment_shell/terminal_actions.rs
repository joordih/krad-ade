use gpui_kit::base::TextSelection;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, ClipboardItem, Context, IntoElement, ParentElement, Styled, Window,
};
use krad_domain::{AgentPreset, TerminalId};
use krad_runtime::{paste_bytes, TerminalAction};
use krad_session::SearchDirection;

use super::live_terminal::terminal_ref;
use super::sessions::Launch;
use super::EnvironmentShell;
use crate::actions::{CopySelection, PasteClipboard, SearchTerminal, SplitTerminal};
use crate::components::{icon, Button, IconName};
use crate::theme::{
    floating_shadow, focus_ring, palette, ICON, INPUT_H, LINE_SM, RADIUS_SM, SPACE_1, SPACE_2,
    TEXT_SM, TEXT_XS,
};

const SEARCH_PLACEHOLDER: &str = "Search terminal";
const SEARCH_NO_MATCH: &str = "No matches";
const SEARCH_HINT: &str = "↵ older · ⇧↵ newer · esc close";
const SEARCH_CARET: &str = "▏";
const BTN_SEARCH_OLDER: &str = "search-older";
const BTN_SEARCH_NEWER: &str = "search-newer";
const BTN_SEARCH_CLOSE: &str = "search-close";
const SEARCH_BAR_W: f32 = 420.0;
const SEARCH_BAR_INSET: f32 = 12.0;
const KEY_ESCAPE: &str = "escape";
const KEY_ENTER: &str = "enter";
const KEY_BACKSPACE: &str = "backspace";

#[derive(Default)]
pub(super) struct SearchQuery {
    text: String,
    missing: bool,
}

impl EnvironmentShell {
    pub(super) fn on_copy_selection(
        &mut self,
        _: &CopySelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chosen = TextSelection::selected_text(window, cx).trim().to_owned();
        if !chosen.is_empty() {
            return Self::copy_text(Some(chosen), cx);
        }
        if !self.runtime.settings.copy_mode.copies_on_shortcut() {
            return;
        }
        let focused = self.envs.get(&self.active).and_then(|live| live.focused);
        if let Some(id) = focused {
            self.copy_terminal_selection(id, cx);
        }
    }

    pub(super) fn copy_finished_selection(&self, id: TerminalId, cx: &mut Context<Self>) {
        if self.runtime.settings.copy_mode.copies_on_select() {
            self.copy_terminal_selection(id, cx);
        }
    }

    fn copy_terminal_selection(&self, id: TerminalId, cx: &mut Context<Self>) {
        let runs = self
            .envs
            .get(&self.active)
            .is_some_and(|live| live.sessions.contains_key(&id));
        if !runs {
            return;
        }
        let selection = self
            .terminals()
            .terminal_selection(terminal_ref(&self.active, id));
        cx.spawn(async move |_, cx| {
            let Ok(Some(text)) = selection.await else {
                return;
            };
            cx.update(|cx| Self::copy_text(Some(text), cx));
        })
        .detach();
    }

    fn copy_text(text: Option<String>, cx: &mut gpui_kit::App) {
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    pub(super) fn on_paste_clipboard(
        &mut self,
        _: &PasteClipboard,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.paste_into_focused(&text, cx);
    }

    pub(super) fn paste_into(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        if let Some(live) = self.envs.get_mut(&self.active) {
            live.focused = Some(id);
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.paste_into_focused(&text, cx);
        }
        cx.notify();
    }

    pub(super) fn paste_into_focused(&mut self, text: &str, cx: &mut Context<Self>) {
        let bracketed = self
            .focused_session()
            .is_some_and(|session| session.view.status().bracketed_paste);
        self.send_to_focused(paste_bytes(text, bracketed), cx);
    }

    pub(super) fn on_search_terminal(
        &mut self,
        _: &SearchTerminal,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search = Some(SearchQuery::default());
        cx.notify();
    }

    pub(super) fn on_split_terminal(
        &mut self,
        _: &SplitTerminal,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let anchor = self.envs.get(&self.active).and_then(|live| live.focused);
        let shell = AgentPreset::Shell;
        self.spawn_terminal(shell.display_name(), Launch::Preset(shell), anchor, cx);
    }

    pub(super) fn ingest_search_key(
        &mut self,
        key: &str,
        key_char: Option<&str>,
        shift: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(search) = &mut self.search else {
            return false;
        };
        match key {
            KEY_ESCAPE => {
                self.search = None;
                if let Some(id) = self.envs.get(&self.active).and_then(|live| live.focused) {
                    let env = self.active.clone();
                    self.act_on_terminal(&env, id, TerminalAction::ClearSelection);
                }
            }
            KEY_ENTER => {
                let direction = if shift {
                    SearchDirection::Newer
                } else {
                    SearchDirection::Older
                };
                self.run_search(direction, false, cx);
            }
            KEY_BACKSPACE => {
                search.text.pop();
                self.run_search(SearchDirection::Older, true, cx);
            }
            _ => {
                let Some(text) = key_char.filter(|text| !text.is_empty()) else {
                    return true;
                };
                search.text.push_str(text);
                self.run_search(SearchDirection::Older, true, cx);
            }
        }
        cx.notify();
        true
    }

    fn run_search(&mut self, direction: SearchDirection, restart: bool, cx: &mut Context<Self>) {
        let Some(query) = self.search.as_ref().map(|search| search.text.clone()) else {
            return;
        };
        let focused = self
            .focused_session()
            .and(self.envs.get(&self.active).and_then(|live| live.focused));
        let Some(id) = focused else {
            if let Some(search) = &mut self.search {
                search.missing = !query.is_empty();
            }
            return;
        };
        let found = self.terminals().search_terminal(
            terminal_ref(&self.active, id),
            query.clone(),
            direction,
            restart,
        );
        cx.spawn(async move |this, cx| {
            let found = found.await.unwrap_or(false);
            this.update(cx, |shell, cx| {
                if let Some(search) = shell.search.as_mut().filter(|search| search.text == query) {
                    search.missing = !query.is_empty() && !found;
                    cx.notify();
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn search_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(search) = &self.search else {
            return div().into_any_element();
        };
        let pal = palette(cx);
        let status = if search.missing {
            SEARCH_NO_MATCH
        } else {
            SEARCH_HINT
        };
        let empty = search.text.is_empty();
        let query = if empty {
            SEARCH_PLACEHOLDER.to_owned()
        } else {
            format!("{}{SEARCH_CARET}", search.text)
        };
        div()
            .absolute()
            .top(px(SEARCH_BAR_INSET))
            .right(px(SEARCH_BAR_INSET))
            .w(px(SEARCH_BAR_W))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(INPUT_H))
            .pl(px(SPACE_2))
            .pr(px(SPACE_1))
            .bg(rgb(pal.bg_surface))
            .border_1()
            .border_color(rgb(pal.accent))
            .rounded(px(RADIUS_SM))
            .shadow(
                focus_ring(pal)
                    .into_iter()
                    .chain(floating_shadow(pal))
                    .collect(),
            )
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .child(icon(IconName::Search, ICON, pal.text_subtle))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(if empty {
                        pal.text_disabled
                    } else {
                        pal.text_strong
                    }))
                    .child(query),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .when(search.missing, |hint| hint.text_color(rgb(pal.error_fg)))
                    .when(!search.missing, |hint| {
                        hint.text_color(rgb(pal.text_subtle))
                    })
                    .child(status),
            )
            .child(
                Button::new(BTN_SEARCH_OLDER)
                    .icon(IconName::ChevronUp)
                    .small()
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.run_search(SearchDirection::Older, false, cx);
                        cx.notify();
                    })),
            )
            .child(
                Button::new(BTN_SEARCH_NEWER)
                    .icon(IconName::ChevronDown)
                    .small()
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.run_search(SearchDirection::Newer, false, cx);
                        cx.notify();
                    })),
            )
            .child(
                Button::new(BTN_SEARCH_CLOSE)
                    .icon(IconName::Close)
                    .small()
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.search = None;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
