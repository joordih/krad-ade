use gpui_kit::Context;
use krad_chat::LiveSession;
use krad_domain::{AgentPreset, SessionId, SessionLead, TerminalId};

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
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let model = structured_mut(self, &env, id).and_then(|chat| chat.chosen_model.clone());
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let Some(session_id) = live.data.session_of(id) else {
            return;
        };
        let resume = live
            .data
            .nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.resume.clone());
        let Some(session) = live
            .data
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return;
        };
        let leads = session
            .lead
            .as_ref()
            .is_some_and(|lead| lead.terminal == Some(id));
        session.lead = (!leads).then(|| SessionLead {
            terminal: Some(id),
            ..SessionLead::new(AgentPreset::ClaudeCode, model)
        });
        if let Some(chat) = structured_mut(self, &env, id) {
            if let Some(resume) = resume {
                chat.session = LiveSession::Resume(resume);
            }
            chat.live = None;
        }
        self.store();
        cx.notify();
    }

    pub(in crate::environment_shell) fn dismiss_lead(
        &mut self,
        id: SessionId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            live.data.dismiss_lead(id);
        }
        cx.notify();
        self.store();
    }
}
