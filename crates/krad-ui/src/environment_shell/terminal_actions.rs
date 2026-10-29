use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, ClipboardItem, Context, IntoElement, ParentElement, Styled, Window,
};
use krad_domain::{AgentPreset, TerminalId};
use krad_session::SearchDirection;

use super::sessions::Launch;
use super::EnvironmentShell;
use crate::actions::{CopySelection, PasteClipboard, SearchTerminal, SplitTerminal};
use crate::components::{icon, Button, IconName};
use crate::theme::{
    floating_shadow, focus_ring, palette, ICON, INPUT_H, LINE_SM, RADIUS_SM, SPACE_1, SPACE_2,
    TEXT_SM, TEXT_XS,
};

const BRACKETED_PASTE_START: &[u8] = b"\x1b[200~";
const BRACKETED_PASTE_END: &[u8] = b"\x1b[201~";
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

pub(super) fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    if !bracketed {
        return text.as_bytes().to_vec();
    }
    [BRACKETED_PASTE_START, text.as_bytes(), BRACKETED_PASTE_END].concat()
}

#[derive(Default)]
pub(super) struct SearchQuery {
    text: String,
    missing: bool,
}

impl EnvironmentShell {
    pub(super) fn on_copy_selection(
        &mut self,
        _: &CopySelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.copy_mode.copies_on_shortcut() {
            return;
        }
        let text = self
            .focused_session()
            .and_then(|session| session.engine.selection_text());
        Self::copy_text(text, cx);
    }

    pub(super) fn copy_finished_selection(&self, id: TerminalId, cx: &mut Context<Self>) {
        if !self.settings.copy_mode.copies_on_select() {
            return;
        }
        let text = self
            .envs
            .get(&self.active)
            .and_then(|live| live.sessions.get(&id))
            .and_then(|session| session.engine.selection_text());
        Self::copy_text(text, cx);
    }

    fn copy_text(text: Option<String>, cx: &mut Context<Self>) {
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
            .is_some_and(|session| session.engine.bracketed_paste());
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
                if let Some(session) = self.focused_session_mut() {
                    session.engine.clear_selection();
                }
            }
            KEY_ENTER => {
                let direction = if shift {
                    SearchDirection::Newer
                } else {
                    SearchDirection::Older
                };
                self.run_search(direction, false);
            }
            KEY_BACKSPACE => {
                search.text.pop();
                self.run_search(SearchDirection::Older, true);
            }
            _ => {
                let Some(text) = key_char.filter(|text| !text.is_empty()) else {
                    return true;
                };
                search.text.push_str(text);
                self.run_search(SearchDirection::Older, true);
            }
        }
        cx.notify();
        true
    }

    fn run_search(&mut self, direction: SearchDirection, restart: bool) {
        let Some(query) = self.search.as_ref().map(|search| search.text.clone()) else {
            return;
        };
        let found = self.focused_session_mut().is_some_and(|session| {
            if restart {
                session.engine.clear_selection();
            }
            !query.is_empty() && session.engine.search(&query, direction)
        });
        if let Some(search) = &mut self.search {
            search.missing = !query.is_empty() && !found;
        }
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
                        this.run_search(SearchDirection::Older, false);
                        cx.notify();
                    })),
            )
            .child(
                Button::new(BTN_SEARCH_NEWER)
                    .icon(IconName::ChevronDown)
                    .small()
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.run_search(SearchDirection::Newer, false);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pastes_are_bracketed_only_when_the_program_asked() {
        assert_eq!(paste_bytes("fix it", false), b"fix it");
        assert_eq!(paste_bytes("fix it", true), b"\x1b[200~fix it\x1b[201~");
    }
}
