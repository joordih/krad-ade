use gpui_kit::{Context, Window};
use krad_domain::{AttentionTarget, EnvironmentView, ToastKind};

use super::super::EnvironmentShell;
use crate::actions::NextWaitingAgent;

const NOTHING_WAITING: &str = "No agent is waiting for you";
const AGENT_GONE: &str = "That agent is no longer open";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn on_next_waiting_agent(
        &mut self,
        _: &NextWaitingAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_next_waiting(window, cx);
    }

    pub(in crate::environment_shell) fn open_next_waiting(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flush_attention(cx);
        let current = self
            .envs
            .get(&self.active)
            .and_then(|live| live.focused)
            .map(|id| AttentionTarget::new(self.active.clone(), id));
        let next = self.attention.queue.next_after(current.as_ref()).cloned();
        match next {
            Some(target) => self.open_attention(&target, window, cx),
            None => self.push_toast(ToastKind::Warning, NOTHING_WAITING.to_owned()),
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn open_attention(
        &mut self,
        target: &AttentionTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self.envs.get(&target.environment).is_some_and(|live| {
            live.data
                .nodes
                .iter()
                .any(|node| node.id == target.terminal)
        });
        if !open {
            self.push_toast(ToastKind::Warning, AGENT_GONE.to_owned());
            cx.notify();
            return;
        }
        if self.active != target.environment {
            self.switch_env(&target.environment, cx);
        }
        if self.active != target.environment {
            return;
        }
        self.set_view(EnvironmentView::Sessions, cx);
        self.show_terminal_tab(target.terminal, window, cx);
    }
}
