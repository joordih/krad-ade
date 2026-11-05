use gpui_kit::{
    div, px, rgb, AnyView, AppContext as _, Context, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Render, Styled, WeakEntity, Window,
};
use krad_domain::{EnvironmentName, TerminalId};
use krad_extensions::api::TerminalHandle;

use super::super::super::terminal_body::{
    embedded_screen, BodyGeometry, BodyLook, TERMINAL_FONT_PX, TERMINAL_LINE_PX,
};
use super::super::super::EnvironmentShell;
use super::super::SlotKey;
use super::ViewState;
use crate::terminal_theme::terminal_colors;
use crate::theme::{mono_family, palette, RADIUS_XS, TEXT_SM};

const VIEW_TERMINAL_ID: &str = "extension-view-terminal";
const VIEW_TERMINAL_MIN_H: f32 = 160.0;
const UNZOOMED: f32 = 1.0;
const STARTING_TEXT: &str = "starting…";

pub(in crate::environment_shell) struct ViewTerminal {
    shell: WeakEntity<EnvironmentShell>,
    env: EnvironmentName,
    terminal: TerminalId,
    focus: FocusHandle,
}

impl ViewTerminal {
    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let (env, terminal) = (self.env.clone(), self.terminal);
        self.shell
            .update(cx, |shell, cx| {
                shell.type_into_view_terminal(&env, terminal, &event.keystroke);
                cx.notify();
            })
            .unwrap_or_default();
    }
}

impl Render for ViewTerminal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        let frame = div()
            .id(VIEW_TERMINAL_ID)
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .w_full()
            .min_h(px(VIEW_TERMINAL_MIN_H))
            .rounded(px(RADIUS_XS))
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&this.focus, cx);
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.key_down(event, cx)));
        let Some(shell) = self.shell.upgrade() else {
            return frame;
        };
        let shell = shell.read(cx);
        let Some(session) = shell
            .envs
            .get(&self.env)
            .and_then(|live| live.sessions.get(&self.terminal))
        else {
            let pal = palette(cx);
            return frame
                .text_size(px(TEXT_SM))
                .text_color(rgb(pal.text_muted))
                .child(STARTING_TEXT);
        };
        let mono = mono_family(&shell.settings.terminal_font);
        let look = BodyLook {
            geometry: BodyGeometry {
                origin: (0.0, 0.0),
                cell_w: shell.cell_width_px.unwrap_or(TERMINAL_FONT_PX),
                line_h: TERMINAL_LINE_PX,
            },
            ligatures: shell.settings.terminal_font.ligatures,
            zoom: UNZOOMED,
            text_zoom: UNZOOMED,
            focused,
            corner: RADIUS_XS,
        };
        let screen = embedded_screen(session, look, &mono, &terminal_colors(cx), cx);
        frame.child(screen)
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn attach_view_terminal(
        &mut self,
        key: &SlotKey,
        handle: TerminalHandle,
        env: &EnvironmentName,
        terminal: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let shell = cx.entity().downgrade();
        let env = env.clone();
        let embedded: AnyView = cx
            .new(|cx| ViewTerminal {
                shell,
                env,
                terminal,
                focus: cx.focus_handle(),
            })
            .into();
        self.extensions
            .view_terminals
            .push((key.clone(), handle, embedded.clone()));
        let shown: Vec<_> = self
            .extensions
            .views
            .views
            .values()
            .filter(|open| open.key == *key)
            .filter_map(|open| match &open.state {
                ViewState::Shown { entity, .. } => Some(entity.clone()),
                ViewState::Loading | ViewState::Failed(_) => None,
            })
            .collect();
        for entity in shown {
            let embedded = embedded.clone();
            entity.update(cx, |view, cx| view.set_terminal(handle, embedded, cx));
        }
    }
}
