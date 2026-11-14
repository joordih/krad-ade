use gpui_kit::{Context, Window};
use krad_domain::{new_session_id, CloudRunnerId, LiveSession, RuntimeId, TerminalId, ToastKind};
use krad_protocol::{ChatCommand, ChatMirror, ChatPage, ChatSummary, Command, Reply};

use super::super::super::problems::toast_kind;
use super::super::super::EnvironmentShell;
use super::super::shown_images::ShownImages;
use super::super::{Backend, ChatPlace, ChatView};
use super::{ChatDriver, StructuredChat};

const UNREACHED: &str = "The chat's runtime is out of reach; nothing was sent";

pub(in crate::environment_shell) fn mirrored_summary(
    place: &ChatPlace,
    id: TerminalId,
    mirror: ChatMirror,
) -> ChatSummary {
    let ChatMirror {
        state,
        from,
        messages,
        asks,
    } = mirror;
    ChatSummary {
        state,
        tail: ChatPage {
            chat: place.chat(id),
            len: from + messages.len(),
            from,
            messages,
        },
        asks,
    }
}

fn refusal_toast(reply: Reply) -> Option<(ToastKind, String)> {
    match reply {
        Reply::Accepted => None,
        Reply::Refused { code, message } => Some((toast_kind(code), message)),
    }
}

fn hosted_structured(mirror: &ChatMirror) -> StructuredChat {
    let state = &mirror.state;
    let session = state
        .resume
        .clone()
        .map_or_else(|| LiveSession::Start(new_session_id()), LiveSession::Resume);
    let mut structured = StructuredChat::new(session, state.harness);
    structured.driver = ChatDriver::InRuntime;
    structured.chosen_model.clone_from(&state.chosen_model);
    structured.effort = state.effort;
    structured.permission_mode = state.permission_mode;
    structured
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_hosted_chat(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(runner) = place.runner() else {
            return;
        };
        let opened = self
            .live(place)
            .is_some_and(|live| live.chats.contains_key(&id));
        if opened {
            return;
        }
        let mirror = self
            .runtime
            .view
            .read(cx)
            .remotes
            .chat(runner, &place.chat(id))
            .map(|chat| chat.mirror.clone());
        let Some(mirror) = mirror else {
            return;
        };
        let (input, events) = Self::chat_input(id, window, cx);
        let structured = hosted_structured(&mirror);
        let mut chat = ChatView::new(input, events, Backend::Structured(Box::new(structured)));
        chat.images = ShownImages::fetched();
        let Some(live) = self.live_mut(place) else {
            return;
        };
        live.chats.insert(id, chat);
        let summary = mirrored_summary(place, id, mirror);
        self.runtime_chats_replaced(place.runtime, vec![summary], cx);
        self.fetch_shown_images(place, id, cx);
    }

    pub(in crate::environment_shell) fn reseed_hosted_chats(
        &mut self,
        runner: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        let places: Vec<ChatPlace> = self
            .hosted_lives()
            .places()
            .filter(|place| place.runner() == Some(runner))
            .cloned()
            .collect();
        for place in places {
            let opened: Vec<TerminalId> = self
                .live(&place)
                .map_or_else(Vec::new, |live| live.chats.keys().copied().collect());
            let view = self.runtime.view.read(cx);
            let summaries: Vec<ChatSummary> = opened
                .into_iter()
                .filter_map(|id| {
                    let mirror = view.remotes.chat(runner, &place.chat(id))?.mirror.clone();
                    Some(mirrored_summary(&place, id, mirror))
                })
                .collect();
            let ids: Vec<TerminalId> = summaries
                .iter()
                .map(|summary| summary.tail.chat.terminal)
                .collect();
            self.runtime_chats_replaced(RuntimeId::Runner(runner), summaries, cx);
            for id in ids {
                self.fetch_shown_images(&place, id, cx);
            }
        }
    }

    pub(in crate::environment_shell) fn send_chat(
        &mut self,
        place: &ChatPlace,
        command: ChatCommand,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.chat_client(place).cloned() else {
            return self.toast(ToastKind::Warning, UNREACHED, cx);
        };
        let reply = client.send(Command::Chat(command));
        cx.spawn(async move |this, cx| {
            let Some((kind, message)) = reply.await.ok().and_then(refusal_toast) else {
                return;
            };
            this.update(cx, |shell, cx| shell.toast(kind, &message, cx))
                .unwrap_or_default();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests;
