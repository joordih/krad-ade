use gpui_kit::{
    div, AnyElement, Context, InteractiveElement, IntoElement, KeyDownEvent, Modifiers,
    ParentElement, Styled, Window,
};
use krad_domain::TerminalId;
use krad_protocol::WriteBy;
use krad_runtime::TerminalAction;
use krad_session::KeyModifiers;

use super::live_terminal::LiveTerminal;
use super::rename::KEY_ESCAPE;
use super::EnvironmentShell;
use crate::actions::{SendBackTab, SendTab, SHELL_KEY_CONTEXT};
use crate::typed_text::typed_text;

const TAB: &[u8] = b"\t";
const BACK_TAB: &[u8] = b"\x1b[Z";

fn is_dead_key(event: &KeyDownEvent) -> bool {
    event.prefer_character_input && !event.keystroke.modifiers.control
}

impl EnvironmentShell {
    pub(super) fn focused_session(&self) -> Option<&LiveTerminal> {
        let live = self.envs.get(&self.active)?;
        live.sessions.get(&live.focused?)
    }

    fn focused_terminal(&self) -> Option<TerminalId> {
        let live = self.envs.get(&self.active)?;
        live.focused.filter(|id| live.sessions.contains_key(id))
    }

    pub(super) fn send_to_focused(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        let Some(focused) = self.focused_terminal() else {
            return;
        };
        let env = self.active.clone();
        self.write_terminal(&env, focused, bytes, WriteBy::User);
        cx.notify();
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
        let Some(focused) = self.focused_terminal() else {
            return;
        };
        let env = self.active.clone();
        let action = TerminalAction::Key {
            key: key.to_owned(),
            text: key_char.map(str::to_owned),
            held,
        };
        self.act_on_terminal(&env, focused, action);
        cx.notify();
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
            .on_action(cx.listener(Self::on_show_tab_at))
            .on_action(cx.listener(Self::on_run_extension_command))
            .on_action(cx.listener(Self::on_navigate_back))
            .on_action(cx.listener(Self::on_navigate_forward))
            .on_action(cx.listener(Self::on_copy_selection))
            .on_action(cx.listener(Self::on_paste_clipboard))
            .on_action(cx.listener(Self::on_search_terminal))
            .on_action(cx.listener(Self::on_split_terminal))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(Self::on_close_all_tabs))
            .on_action(cx.listener(Self::on_toggle_activity))
            .on_action(cx.listener(Self::on_toggle_sidebar))
            .on_action(cx.listener(Self::on_toggle_side_panel))
            .on_action(cx.listener(Self::on_toggle_minimap))
            .on_action(cx.listener(Self::on_toggle_work_mode))
            .on_action(cx.listener(Self::on_quick_open))
            .on_action(cx.listener(Self::on_find_in_files))
            .on_action(cx.listener(Self::on_open_settings))
            .on_action(cx.listener(Self::on_text_larger))
            .on_action(cx.listener(Self::on_text_smaller))
            .on_action(cx.listener(Self::on_text_reset))
            .on_action(cx.listener(Self::on_new_agent))
            .on_action(cx.listener(Self::on_next_waiting_agent))
            .on_action(cx.listener(Self::on_toggle_attention_inbox))
            .on_action(cx.listener(Self::on_toggle_kraddies))
            .on_action(cx.listener(Self::on_toggle_automations))
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
                    let typed = typed_text(stroke, window.capslock());
                    this.ingest_key(&stroke.key, typed.as_deref(), stroke.modifiers, cx);
                }),
            )
            .child(view)
            .child(self.search_bar(cx))
            .into_any_element()
    }
}
