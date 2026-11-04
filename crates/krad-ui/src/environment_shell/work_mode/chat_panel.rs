use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, px, AnyElement, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement,
    PinchEvent, Pixels, Point, StatefulInteractiveElement, Styled,
};
use krad_domain::TerminalId;
use std::rc::Rc;

use super::super::context_menu::{MenuAction, MenuEntry, MenuSearch};
use super::super::session_pane::{
    divides, tab_capsule, tab_divider, tab_picker_shell, DraggedTab, TabWidth,
};
use super::super::{EnvironmentShell, LiveEnv};
use crate::components::{Button, ButtonVariant, CenteredEmpty, IconName};
use crate::theme::{palette, LINE_SM, SPACE_0_5, TAB_STRIP_H, TAB_STRIP_MARGIN, TEXT_SM};

const BTN_NEW_CHAT: &str = "editor-chat-new";
const BTN_PICK_CHAT: &str = "editor-chat-pick";
const PICK_PLACEHOLDER: &str = "Filter chats…";
const STRIP_ID: &str = "editor-chat-strip";
const BTN_EMPTY_NEW_CHAT: &str = "editor-chat-empty-new";
const NEW_CHAT_TOOLTIP: &str = "New chat";
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

    fn chat_picker_entries(&self, live: &LiveEnv) -> Vec<MenuEntry> {
        let shown = live.data.editor_chat();
        live.data
            .session_chats()
            .into_iter()
            .filter_map(|id| {
                let node = live.data.nodes.iter().find(|node| node.id == id)?;
                let entry = MenuEntry::checked(
                    self.title_label(id, &node.title),
                    shown == Some(id),
                    MenuAction::ShowEditorChat(id),
                );
                Some(entry)
            })
            .collect()
    }

    fn editor_chat_strip(&self, live: &LiveEnv, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let chats = live.data.session_chats();
        let shown = live.data.editor_chat();
        let shown_index = shown.and_then(|shown| chats.iter().position(|id| *id == shown));
        let mut tabs: Vec<AnyElement> = Vec::new();
        for (index, id) in chats.iter().copied().enumerate() {
            if index > 0 && divides(index - 1, shown_index) {
                tabs.push(tab_divider(pal));
            }
            tabs.extend(self.terminal_tab(live, id, shown == Some(id), cx));
        }
        let entries = Rc::new(self.chat_picker_entries(live));
        let footer = vec![
            MenuEntry::Separator,
            MenuEntry::item(
                NEW_CHAT_LABEL,
                Some(IconName::Add),
                MenuAction::NewEditorChat,
            ),
        ];
        let search = Rc::new(MenuSearch {
            placeholder: PICK_PLACEHOLDER,
            create: None,
            footer,
        });
        div()
            .id(STRIP_ID)
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_0_5))
            .h(px(TAB_STRIP_H))
            .px(px(TAB_STRIP_MARGIN))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .on_drop(cx.listener(|this: &mut Self, dragged: &DraggedTab, _, cx| {
                this.move_chat_to_end(dragged.id, cx);
            }))
            .child(tab_picker_shell(BTN_PICK_CHAT, cx).on_click(cx.listener(
                move |this: &mut Self, event: &ClickEvent, window, cx| {
                    let Point { x, y }: Point<Pixels> = event.position();
                    this.open_searchable_menu(
                        (x.as_f32(), y.as_f32()),
                        entries.to_vec(),
                        (*search).clone(),
                        window,
                        cx,
                    );
                },
            )))
            .child(tab_capsule(tabs, TabWidth::Fill, pal))
            .child(
                Button::new(BTN_NEW_CHAT)
                    .icon(IconName::Add)
                    .small()
                    .tooltip(NEW_CHAT_TOOLTIP)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.add_chat_tab(cx))),
            )
            .into_any_element()
    }

    fn move_chat_to_end(&mut self, chat: TerminalId, cx: &mut Context<Self>) {
        let Some(pane) = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.current_session())
            .and_then(|session| session.panes.pane_of(chat))
        else {
            return;
        };
        self.edit_panes(cx, |panes| {
            panes.move_tab(chat, pane, usize::MAX).unwrap_or_default();
        });
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
        let Some(live) = self.envs.get(&self.active) else {
            return div().into_any_element();
        };
        let shown = live
            .data
            .editor_chat()
            .and_then(|id| live.chats.get(&id).map(|chat| (id, chat)));
        let body = if let Some((id, chat)) = shown {
            self.surface_chat_body(id, chat, cx)
        } else {
            Self::no_editor_chat(cx)
        };
        let header = self.editor_chat_strip(live, cx);
        let pinched = shown.map(|(id, _)| id);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .when_some(pinched, |panel, id| {
                panel.on_pinch(
                    cx.listener(move |this: &mut Self, event: &PinchEvent, _, cx| {
                        this.pinch_tab(id, event, cx);
                    }),
                )
            })
            .child(header)
            .child(div().flex().flex_col().flex_1().min_h_0().child(body))
            .into_any_element()
    }
}
