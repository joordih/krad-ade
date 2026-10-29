use crate::environment::EnvironmentData;
use crate::ids::{GroupId, SessionId};

const NEW_SESSION_PREFIX: &str = "Session";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionSpec {
    pub name: String,
}

impl EnvironmentData {
    pub fn new_session(&mut self, group: Option<GroupId>, spec: SessionSpec) -> SessionId {
        let ordinal = self.sessions_of(group).count() + 1;
        let id = self.push_session(group, format!("{NEW_SESSION_PREFIX} {ordinal}"));
        self.apply_spec(id, spec);
        id
    }

    fn apply_spec(&mut self, id: SessionId, spec: SessionSpec) {
        let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) else {
            return;
        };
        let SessionSpec { name } = spec;
        let name = name.trim();
        if !name.is_empty() {
            name.clone_into(&mut session.name);
        }
        self.active_session = Some(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::seed_environment;
    use std::path::PathBuf;

    #[test]
    fn a_new_session_takes_its_name_or_a_number() {
        let mut environment = seed_environment();
        let group = environment.add_group(PathBuf::from("/work/api"), None);
        environment.enter_sessions();
        environment.new_session(Some(group), SessionSpec::default());
        assert_eq!(
            environment
                .current_session()
                .map(|session| session.name.as_str()),
            Some("Session 1")
        );
        let named = SessionSpec {
            name: "  Refund flow ".to_owned(),
        };
        let id = environment.new_session(Some(group), named);
        assert_eq!(environment.active_session, Some(id));
        assert_eq!(
            environment
                .current_session()
                .map(|session| session.name.as_str()),
            Some("Refund flow")
        );
    }
}
