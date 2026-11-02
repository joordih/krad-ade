use gpui_kit::{
    div, px, rgb, AnyElement, ClickEvent, Context, FontWeight, IntoElement, ParentElement, Pixels,
    Point, Styled,
};
use krad_domain::TerminalId;

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::EnvironmentShell;
use crate::components::{icon, Button, ButtonVariant, CenteredEmpty, IconName};
use crate::theme::{palette, CARD_HEADER_H, ICON_SM, LINE_SM, SPACE_1_5, SPACE_2, TEXT_SM};

const BTN_NEW_CHAT: &str = "editor-chat-new";
const BTN_PICK_CHAT: &str = "editor-chat-pick";
const BTN_EMPTY_NEW_CHAT: &str = "editor-chat-empty-new";
const NEW_CHAT_TOOLTIP: &str = "New chat";
const PICK_CHAT_TOOLTIP: &str = "Chats of this session";
const NEW_CHAT_LABEL: &str = "New chat";
const EMPTY_TITLE: &str = "No chat in this session";
const EMPTY_TEXT: &str = "Start one to talk with an agent beside your files.";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn show_editor_chat(
        &mut self,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.envs.get_mut(&self.active) {
            live.data.remember_chat(id);
        }
        cx.notify();
        self.store();
    }

    fn open_chat_picker(&mut self, event: &ClickEvent, cx: &mut Context<Self>) {
        let Some(live) = self.envs.get(&self.active) else {
            return;
        };
        let shown = live.data.editor_chat();
        let entries: Vec<MenuEntry> = live
            .data
            .session_chats()
            .into_iter()
            .filter_map(|id| {
                let node = live.data.nodes.iter().find(|node| node.id == id)?;
                Some(MenuEntry::checked(
                    self.title_label(id, &node.title),
                    shown == Some(id),
                    MenuAction::ShowEditorChat(id),
                ))
            })
            .collect();
        let Point { x, y }: Point<Pixels> = event.position();
        self.open_menu((x.as_f32(), y.as_f32()), MenuPlacement::Below, entries, cx);
    }

    fn no_editor_chat(cx: &mut Context<Self>) -> AnyElement {
        let start = Button::new(BTN_EMPTY_NEW_CHAT)
            .label(NEW_CHAT_LABEL)
            .icon(IconName::Add)
            .variant(ButtonVariant::Primary)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| this.add_chat_tab(cx)));
        CenteredEmpty::new(EMPTY_TITLE, EMPTY_TEXT)
            .action(start)
            .into_any_element()
    }

    pub(in crate::environment_shell) fn editor_chat_panel(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let Some(live) = self.envs.get(&self.active) else {
            return div().into_any_element();
        };
        let shown = live
            .data
            .editor_chat()
            .and_then(|id| live.chats.get(&id).map(|chat| (id, chat)));
        let title = shown
            .and_then(|(id, _)| {
                let node = live.data.nodes.iter().find(|node| node.id == id)?;
                Some(self.title_label(id, &node.title))
            })
            .unwrap_or_default();
        let body = if let Some((id, chat)) = shown {
            self.surface_chat_body(id, chat, cx)
        } else {
            Self::no_editor_chat(cx)
        };
        let header = div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(CARD_HEADER_H))
            .px(px(SPACE_2))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(pal.text_default))
            .child(icon(IconName::Chat, ICON_SM, pal.text_muted))
            .child(div().flex_1().min_w_0().truncate().child(title))
            .child(
                Button::new(BTN_PICK_CHAT)
                    .icon(IconName::ChevronDown)
                    .small()
                    .tooltip(PICK_CHAT_TOOLTIP)
                    .on_click(cx.listener(|this: &mut Self, event: &ClickEvent, _, cx| {
                        this.open_chat_picker(event, cx);
                    })),
            )
            .child(
                Button::new(BTN_NEW_CHAT)
                    .icon(IconName::Add)
                    .small()
                    .tooltip(NEW_CHAT_TOOLTIP)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.add_chat_tab(cx))),
            );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(header)
            .child(div().flex().flex_col().flex_1().min_h_0().child(body))
            .into_any_element()
    }
}
