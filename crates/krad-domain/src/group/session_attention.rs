use crate::attention::SessionAttention;
use crate::environment::EnvironmentData;
use crate::ids::{SessionId, TerminalId};

impl EnvironmentData {
    #[must_use]
    pub fn attention_of(&self, terminal: TerminalId) -> SessionAttention {
        self.session_of(terminal)
            .and_then(|id| self.session(id))
            .map_or_else(SessionAttention::default, |session| session.attention)
    }

    pub fn toggle_session_attention(&mut self, id: SessionId) {
        if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
            session.attention = session.attention.toggled();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::attention::SessionAttention;
    use crate::environment::seed_environment;

    #[test]
    fn muting_a_session_mutes_its_terminals_and_toggles_back() {
        let mut environment = seed_environment();
        environment.enter_sessions();
        let terminal = environment.nodes[0].id;
        let session = environment.session_of(terminal);
        assert!(session.is_some());
        let Some(session) = session else {
            return;
        };
        assert_eq!(environment.attention_of(terminal), SessionAttention::Notify);
        environment.toggle_session_attention(session);
        assert_eq!(environment.attention_of(terminal), SessionAttention::Muted);
        environment.toggle_session_attention(session);
        assert_eq!(environment.attention_of(terminal), SessionAttention::Notify);
    }

    #[test]
    fn a_muted_session_is_stored_and_a_notifying_one_is_left_out() -> serde_json::Result<()> {
        let mut environment = seed_environment();
        environment.enter_sessions();
        assert_ne!(environment.sessions.len(), 0);
        let Some(session) = environment.sessions.first_mut() else {
            return Ok(());
        };
        let plain = serde_json::to_string(&*session)?;
        assert!(!plain.contains("attention"));
        session.attention = SessionAttention::Muted;
        let muted = serde_json::to_string(&*session)?;
        assert!(muted.contains(r#""attention":"muted""#));
        Ok(())
    }
}
