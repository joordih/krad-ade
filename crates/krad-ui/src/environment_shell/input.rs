use gpui_kit::{
    div, AnyElement, Context, InteractiveElement, IntoElement, KeyDownEvent, Modifiers,
    ParentElement, Styled, Window,
};
use krad_session::{KeyModifiers, LiveSession, PtyInput, SessionState};

use super::rename::KEY_ESCAPE;
use super::EnvironmentShell;
use crate::actions::{SendBackTab, SendTab, SHELL_KEY_CONTEXT};

const WRITER_LOST_TEXT: &str = "writer lost";
const TAB: &[u8] = b"\t";
const BACK_TAB: &[u8] = b"\x1b[Z";

fn is_dead_key(event: &KeyDownEvent) -> bool {
    event.prefer_character_input && !event.keystroke.modifiers.control
}

impl EnvironmentShell {
    pub(super) fn focused_session(&self) -> Option<&LiveSession> {
        let live = self.envs.get(&self.active)?;
        live.sessions.get(&live.focused?)
    }

    pub(super) fn focused_session_mut(&mut self) -> Option<&mut LiveSession> {
        let live = self.envs.get_mut(&self.active)?;
        let focused = live.focused?;
        live.sessions.get_mut(&focused)
    }

    pub(super) fn send_to_focused(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let Some(focused) = live.focused else {
            return;
        };
        let Some(session) = live.sessions.get_mut(&focused) else {
            return;
        };
        session.engine.scroll_to_bottom();
        cx.notify();
        if session.writer_tx.send(PtyInput::Bytes(bytes)).is_err() {
            if let Some(session) = live.sessions.get_mut(&focused) {
                session.state = SessionState::Failed(WRITER_LOST_TEXT.to_owned());
            }
            cx.notify();
        }
    }

    fn on_send_tab(&mut self, _: &SendTab, window: &mut Window, cx: &mut Context<Self>) {
        self.send_tab_bytes(TAB, window, cx);
    }

    fn on_send_back_tab(&mut self, _: &SendBackTab, window: &mut Window, cx: &mut Context<Self>) {
        self.send_tab_bytes(BACK_TAB, window, cx);
    }

    fn send_tab_bytes(&mut self, bytes: &[u8], window: &mut Window, cx: &mut Context<Self>) {
        if self.focus.is_focused(window) && !self.palette_open {
            self.send_to_focused(bytes.to_vec(), cx);
        } else {
            cx.propagate();
        }
    }

    fn ingest_key(
        &mut self,
        key: &str,
        key_char: Option<&str>,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) {
        if self.palette_open || modifiers.platform {
            return;
        }
        if self.ingest_rename_key(key, key_char, cx) {
            return;
        }
        if self.ingest_search_key(key, key_char, modifiers.shift, cx) {
            return;
        }
        let held = KeyModifiers {
            ctrl: modifiers.control,
            alt: modifiers.alt,
            shift: modifiers.shift,
        };
        let bytes = self
            .focused_session_mut()
            .and_then(|session| session.engine.encode_key(key, key_char, held));
        if let Some(bytes) = bytes {
            self.send_to_focused(bytes, cx);
        }
    }

    pub(super) fn main_pane(&self, view: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        div()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .track_focus(&self.focus)
            .key_context(SHELL_KEY_CONTEXT)
            .on_action(cx.listener(Self::on_toggle_fps))
            .on_action(cx.listener(Self::on_toggle_palette))
            .on_action(cx.listener(Self::on_new_terminal))
            .on_action(cx.listener(Self::on_copy_selection))
            .on_action(cx.listener(Self::on_paste_clipboard))
            .on_action(cx.listener(Self::on_search_terminal))
            .on_action(cx.listener(Self::on_split_terminal))
            .on_action(cx.listener(Self::on_toggle_activity))
            .on_action(cx.listener(Self::on_toggle_sidebar))
            .on_action(cx.listener(Self::on_toggle_side_panel))
            .on_action(cx.listener(Self::on_open_settings))
            .on_action(cx.listener(Self::on_text_larger))
            .on_action(cx.listener(Self::on_text_smaller))
            .on_action(cx.listener(Self::on_text_reset))
            .on_action(cx.listener(Self::on_new_agent))
            .on_action(cx.listener(Self::on_send_tab))
            .on_action(cx.listener(Self::on_send_back_tab))
            .on_key_down(
                cx.listener(|this: &mut Self, event: &KeyDownEvent, window, cx| {
                    let stroke = &event.keystroke;
                    if this.palette_open && stroke.key == KEY_ESCAPE {
                        this.dismiss_palette(window, cx);
                        return;
                    }
                    if !this.focus.is_focused(window) {
                        return;
                    }
                    if is_dead_key(event) {
                        return;
                    }
                    this.ingest_key(
                        &stroke.key,
                        stroke.key_char.as_deref(),
                        stroke.modifiers,
                        cx,
                    );
                }),
            )
            .child(view)
            .child(self.search_bar(cx))
            .into_any_element()
    }
}
