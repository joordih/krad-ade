use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::{
    div, px, relative, rgb, AnyElement, AppContext as _, Context, ElementId, Entity, IntoElement,
    ParentElement, PromptButton, PromptLevel, StatefulInteractiveElement, Styled, Subscription,
    Window,
};
use krad_chat::{RewindScope, Speaker};
use krad_domain::{Mention, Prompt, TerminalId};

use super::super::chat::ChatMessage;
use super::super::EnvironmentShell;
use super::mentions::sent_mentions;
use super::message_actions::action_button;
use super::SurfaceChat;
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{Palette, RADIUS_LG, SPACE_1, SPACE_2, SPACE_3, TEXT_SM};

const EDITOR_ROWS: usize = 3;
const EDITOR_WIDTH: f32 = 0.82;
const EDITOR_PAD_Y: f32 = 10.0;
const EDIT_PREFIX: &str = "surface-question-edit";
const CANCEL_PREFIX: &str = "surface-edit-cancel";
const SEND_PREFIX: &str = "surface-edit-send";
const EDIT_TOOLTIP: &str = "Edit and send again";
const CANCEL_LABEL: &str = "Cancel";
const SEND_LABEL: &str = "Send";
const FILES_TITLE: &str = "Rewind the files too?";
const FILES_DETAIL: &str = "The agent changed files after this message. Both choices remove the later messages; restoring the files puts back what the agent's own edits changed, but not changes made by shell commands.";
const BOTH_LABEL: &str = "Conversation and files";
const CONVERSATION_LABEL: &str = "Conversation only";
const DIALOG_CANCEL_LABEL: &str = "Cancel";
const BOTH_ANSWER: usize = 0;
const CONVERSATION_ANSWER: usize = 1;

pub(super) struct MessageEditor {
    pub(super) index: usize,
    input: Entity<TextareaState>,
    _events: Subscription,
    mentions: Vec<Mention>,
}

pub(super) fn editable(messages: &[ChatMessage], index: usize, idle: bool) -> bool {
    idle && messages
        .get(index)
        .is_some_and(|message| message.speaker == Speaker::User && message.rewind.is_some())
}

pub(super) fn changed_files_after(messages: &[ChatMessage], index: usize) -> bool {
    messages
        .iter()
        .skip(index + 1)
        .flat_map(ChatMessage::tools)
        .any(|tool| tool.edit.is_some())
}

impl SurfaceChat {
    pub(super) fn takes_edits(&self) -> bool {
        self.editing.is_none()
            && self.outbox.messages.is_empty()
            && self
                .structured()
                .is_some_and(|structured| structured.is_free() && !structured.is_rewinding())
    }

    pub(super) fn offers_edit(&self, index: usize) -> bool {
        editable(&self.messages, index, self.takes_edits())
    }
}

impl EnvironmentShell {
    pub(super) fn edit_button(
        id: TerminalId,
        index: usize,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = format!("{EDIT_PREFIX}-{}-{index}", id.as_u64());
        action_button(
            ElementId::Name(key.into()),
            IconName::Pencil,
            EDIT_TOOLTIP,
            pal,
        )
        .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
            this.edit_message(id, index, window, cx);
        }))
        .into_any_element()
    }

    fn edit_message(
        &mut self,
        id: TerminalId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(chat) = self
            .envs
            .get(&env)
            .and_then(|live| live.chats.get(&id))
            .filter(|chat| chat.offers_edit(index))
        else {
            return;
        };
        let Some(message) = chat.messages.get(index) else {
            return;
        };
        let (text, mentions) = (message.text.clone(), message.mentions.clone());
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(EDITOR_ROWS)
                .submit_on_enter(true)
                .default_value(text)
        });
        let events = cx.subscribe_in(
            &input,
            window,
            move |this: &mut Self, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { shift: false, .. } = event {
                    this.send_edit(id, window, cx);
                }
            },
        );
        input.update(cx, |field, cx| field.focus(window, cx));
        if let Some(chat) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.editing = Some(MessageEditor {
                index,
                input,
                _events: events,
                mentions,
            });
            chat.thread.remeasure();
        }
        cx.notify();
    }

    fn cancel_edit(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        let env = self.active.clone();
        if let Some(chat) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.editing = None;
            chat.thread.remeasure();
        }
        cx.notify();
    }

    fn send_edit(&mut self, id: TerminalId, window: &mut Window, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(chat) = self.envs.get(&env).and_then(|live| live.chats.get(&id)) else {
            return;
        };
        let Some(editor) = &chat.editing else {
            return;
        };
        let free = chat.outbox.messages.is_empty()
            && chat
                .structured()
                .is_some_and(|structured| structured.is_free() && !structured.is_rewinding());
        let text = editor.input.read(cx).value().trim().to_owned();
        if !free || (text.is_empty() && editor.mentions.is_empty()) {
            return;
        }
        let index = editor.index;
        let harness = chat.harness();
        let asks_files = harness.restores_files() && changed_files_after(&chat.messages, index);
        let mentions = editor.mentions.clone();
        let text = self.skill_message(harness, text);
        let prompt = Prompt::new(&text, mentions);
        if !asks_files {
            self.start_rewind(&env, id, index, prompt, RewindScope::Conversation, cx);
            return cx.notify();
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            FILES_TITLE,
            Some(FILES_DETAIL),
            &[
                PromptButton::ok(BOTH_LABEL),
                PromptButton::new(CONVERSATION_LABEL),
                PromptButton::cancel(DIALOG_CANCEL_LABEL),
            ],
            cx,
        );
        cx.spawn(async move |this, cx| {
            let scope = match answer.await {
                Ok(BOTH_ANSWER) => RewindScope::ConversationAndFiles,
                Ok(CONVERSATION_ANSWER) => RewindScope::Conversation,
                _ => return,
            };
            this.update(cx, |shell, cx| {
                shell.start_rewind(&env, id, index, prompt, scope, cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn message_editor(
        id: TerminalId,
        chat: &SurfaceChat,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = chat.editing.as_ref()?;
        let rewinding = chat
            .structured()
            .is_some_and(super::structured::StructuredChat::is_rewinding);
        let raw = id.as_u64();
        let buttons = (!rewinding).then(|| {
            div()
                .flex()
                .flex_row()
                .gap(px(SPACE_2))
                .child(
                    Button::new((CANCEL_PREFIX, raw))
                        .label(CANCEL_LABEL)
                        .small()
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.cancel_edit(id, cx);
                        })),
                )
                .child(
                    Button::new((SEND_PREFIX, raw))
                        .label(SEND_LABEL)
                        .small()
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                            this.send_edit(id, window, cx);
                        })),
                )
        });
        Some(
            div()
                .flex()
                .flex_col()
                .w_full()
                .items_end()
                .gap(px(SPACE_1))
                .child(
                    div()
                        .w(relative(EDITOR_WIDTH))
                        .px(px(SPACE_3))
                        .py(px(EDITOR_PAD_Y))
                        .rounded(px(RADIUS_LG))
                        .bg(rgb(pal.bg_panel))
                        .text_size(px(TEXT_SM))
                        .child(
                            Textarea::new(&editor.input)
                                .appearance(false)
                                .disabled(rewinding),
                        ),
                )
                .children(sent_mentions(&editor.mentions, pal))
                .children(buttons)
                .into_any_element(),
        )
    }
}

pub(super) fn is_edited(chat: &SurfaceChat, index: usize) -> bool {
    chat.editing
        .as_ref()
        .is_some_and(|editor| editor.index == index)
}

#[cfg(test)]
mod tests {
    use super::super::super::chat::AnswerPart;
    use super::*;
    use krad_chat::ToolCall;
    use krad_domain::{FileEdit, RewindPoint};

    fn message(speaker: Speaker, rewind: Option<RewindPoint>) -> ChatMessage {
        ChatMessage {
            speaker,
            text: "text".to_owned(),
            parts: Vec::new(),
            error: None,
            running: false,
            mentions: Vec::new(),
            checklist: None,
            rewind,
        }
    }

    #[test]
    fn only_questions_the_harness_can_cut_back_to_offer_edit_while_idle() {
        let messages = [
            message(Speaker::User, None),
            message(Speaker::Assistant, None),
            message(Speaker::User, Some(RewindPoint::Prompt(1))),
            message(Speaker::Assistant, Some(RewindPoint::Prompt(1))),
        ];
        let offered: Vec<bool> = (0..messages.len())
            .map(|index| editable(&messages, index, true))
            .collect();
        assert_eq!(offered, [false, false, true, false]);
        assert!(!editable(&messages, 2, false), "not while the chat works");
        assert!(!editable(&messages, 9, true));
    }

    #[test]
    fn files_are_only_asked_about_when_a_later_answer_edited_one() {
        let mut messages = vec![
            message(Speaker::User, Some(RewindPoint::Prompt(0))),
            message(Speaker::Assistant, None),
        ];
        assert!(!changed_files_after(&messages, 0));
        messages[1].parts.push(AnswerPart::Tool(ToolCall {
            id: None,
            name: "Edit".to_owned(),
            kind: krad_domain::ToolKind::Edit,
            summary: "src/a.rs".to_owned(),
            edit: Some(FileEdit {
                path: "src/a.rs".to_owned(),
                lines: Vec::new(),
            }),
        }));
        assert!(changed_files_after(&messages, 0));
        assert!(!changed_files_after(&messages, 1));
    }
}
