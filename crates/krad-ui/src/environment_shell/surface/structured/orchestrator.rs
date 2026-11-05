use gpui_kit::{Context, Window};
use krad_chat::LiveSession;
use krad_domain::{EnvironmentName, SessionId, SessionLead, TerminalId, DEFAULT_CHAT_HARNESS};

use super::super::super::workers::LeadEnding;
use super::super::super::EnvironmentShell;
use super::structured_mut;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn orchestrates(&self, id: TerminalId) -> bool {
        self.envs
            .get(&self.active)
            .is_some_and(|live| live.data.session_led_by(id).is_some())
    }

    pub(in crate::environment_shell) fn toggle_orchestrator(
        &mut self,
        id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        if let Some(session) = self
            .envs
            .get(&env)
            .and_then(|live| live.data.session_led_by(id))
        {
            self.ask_end_lead(LeadEnding::Dismiss(session), window, cx);
            return;
        }
        let (harness, model) = structured_mut(self, &env, id)
            .map_or((DEFAULT_CHAT_HARNESS, None), |chat| {
                (chat.harness, chat.chosen_model.clone())
            });
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let Some(session_id) = live.data.session_of(id) else {
            return;
        };
        let Some(session) = live
            .data
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return;
        };
        session.lead = Some(SessionLead {
            terminal: Some(id),
            ..SessionLead::new(harness, model)
        });
        self.restart_live_chat(&env, id);
        self.store();
        cx.notify();
    }

    fn restart_live_chat(&mut self, env: &EnvironmentName, id: TerminalId) {
        let resume = self
            .envs
            .get(env)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
            .and_then(|node| node.resume.clone());
        if let Some(chat) = structured_mut(self, env, id) {
            if let Some(resume) = resume {
                chat.session = LiveSession::Resume(resume);
            }
            chat.live = None;
        }
    }

    pub(in crate::environment_shell) fn dismiss_lead(
        &mut self,
        id: SessionId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let lead = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.data.dismiss_lead(id));
        if let Some(terminal) = lead.and_then(|lead| lead.terminal) {
            self.restart_live_chat(&env, terminal);
        }
        cx.notify();
        self.store();
    }
}
