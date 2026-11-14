use gpui_kit::{Context, Window};
use krad_domain::{SessionEdit, SessionId, SessionLead, TerminalId, DEFAULT_CHAT_HARNESS};

use super::super::super::workers::LeadEnding;
use super::super::super::EnvironmentShell;
use super::super::ChatPlace;
use super::structured_mut;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn orchestrates(&self, id: TerminalId) -> bool {
        self.live(&self.chat_place())
            .is_some_and(|live| live.data.session_led_by(id).is_some())
    }

    pub(in crate::environment_shell) fn toggle_orchestrator(
        &mut self,
        id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(env) = self.shown_here() else {
            return;
        };
        if let Some(session) = self
            .envs
            .get(&env)
            .and_then(|live| live.data.session_led_by(id))
        {
            self.ask_end_lead(LeadEnding::Dismiss(session), window, cx);
            return;
        }
        let (harness, model) = structured_mut(self, &ChatPlace::here(&env), id)
            .map_or((DEFAULT_CHAT_HARNESS, None), |chat| {
                (chat.harness, chat.chosen_model.clone())
            });
        let Some(session) = self
            .envs
            .get(&env)
            .and_then(|live| live.data.session_of(id))
        else {
            return;
        };
        let lead = SessionLead {
            terminal: Some(id),
            ..SessionLead::new(harness, model)
        };
        self.edit_environment(&env, SessionEdit::MakeLead { session, lead });
        self.store();
        cx.notify();
    }

    pub(in crate::environment_shell) fn dismiss_lead(
        &mut self,
        id: SessionId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        self.edit_environment(&env, SessionEdit::DismissLead { session: id });
        cx.notify();
        self.store();
    }
}
