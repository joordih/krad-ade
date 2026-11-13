use gpui_kit::{div, px, rgb, App, Context, ParentElement, Styled, Window};
use krad_domain::{AgentState, EnvironmentView, NodeField, SessionEdit, TerminalId, TextScale};

use super::EnvironmentShell;
use crate::actions::{TextLarger, TextReset, TextSmaller};
use crate::theme::{Palette, LINE_XS, SPACE_1, SPACE_3, TEXT_XS};

const HOME_ENV: &str = "HOME";
const USERPROFILE_ENV: &str = "USERPROFILE";

pub(super) fn error_line(text: &str, pal: &Palette) -> gpui_kit::Div {
    div()
        .flex_none()
        .px(px(SPACE_3))
        .py(px(SPACE_1))
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.error_fg))
        .truncate()
        .child(text.to_owned())
}

pub(super) fn home_dir() -> Option<String> {
    std::env::var(HOME_ENV)
        .or_else(|_| std::env::var(USERPROFILE_ENV))
        .ok()
}

impl EnvironmentShell {
    pub(super) fn focus_pane(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
    }

    pub(super) fn focus_terminal(&mut self, id: TerminalId, cx: &App) {
        let env = self.active.clone();
        let shown = self
            .envs
            .get(&env)
            .is_some_and(|live| live.data.session_of(id).is_some());
        if shown {
            self.leave_main_overlays();
        }
        if self
            .edit_environment(&env, SessionEdit::Reveal { terminal: id })
            .applied()
        {
            if let Some(live) = self.envs.get_mut(&env) {
                live.data.set_view(EnvironmentView::Sessions);
            }
        }
        let done = self.envs.get(&env).is_some_and(|live| {
            live.data
                .nodes
                .iter()
                .any(|node| node.id == id && node.state == AgentState::Done)
        });
        if done {
            self.set_node(&env, id, NodeField::State(AgentState::Idle));
        }
        if let Some(live) = self.envs.get_mut(&env) {
            live.focused = Some(id);
            if let Some(group) = live.data.group_of(id) {
                live.selected_group = Some(group.id);
            }
        }
        self.attention_seen(&env, id, cx);
    }

    pub(super) fn step_text_scale(&mut self, id: TerminalId, bigger: bool, cx: &mut Context<Self>) {
        self.set_text_scale(
            id,
            |scale| {
                if bigger {
                    scale.bigger()
                } else {
                    scale.smaller()
                }
            },
            cx,
        );
    }

    pub(super) fn set_text_scale(
        &mut self,
        id: TerminalId,
        change: impl FnOnce(TextScale) -> TextScale,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let scaled = self
            .envs
            .get(&env)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
            .map(|node| (node.text_scale, change(node.text_scale)));
        if let Some((_, scale)) = scaled.filter(|(before, after)| before != after) {
            self.set_node(&env, id, NodeField::TextScale(scale));
        }
        cx.notify();
        self.store();
    }

    pub(super) fn on_text_larger(
        &mut self,
        _: &TextLarger,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.envs.get(&self.active).and_then(|live| live.focused) {
            self.step_text_scale(id, true, cx);
        }
    }

    pub(super) fn on_text_smaller(
        &mut self,
        _: &TextSmaller,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.envs.get(&self.active).and_then(|live| live.focused) {
            self.step_text_scale(id, false, cx);
        }
    }

    pub(super) fn on_text_reset(&mut self, _: &TextReset, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.envs.get(&self.active).and_then(|live| live.focused) {
            self.set_text_scale(id, |_| TextScale::default(), cx);
        }
    }
}
