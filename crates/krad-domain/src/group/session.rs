use super::{SessionLead, SessionLook, SessionPanes};
use crate::environment::{EnvironmentData, TerminalNode};
use crate::ids::{GroupId, SessionId, TerminalId};
use crate::mosaic::MosaicState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DEFAULT_SESSION_NAME: &str = "Main";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub group: Option<GroupId>,
    pub name: String,
    pub tabs: MosaicState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<SessionLead>,
    #[serde(default)]
    pub panes: SessionPanes,
    #[serde(default, skip_serializing_if = "SessionLook::is_plain")]
    pub look: SessionLook,
}

impl EnvironmentData {
    pub fn enter_sessions(&mut self) {
        self.ensure_sessions();
        self.assign_terminals();
    }

    #[must_use]
    pub fn tabs(&self) -> Option<&MosaicState> {
        self.current_session().map(|session| &session.tabs)
    }

    pub fn tabs_mut(&mut self) -> Option<&mut MosaicState> {
        self.current_session_mut().map(|session| &mut session.tabs)
    }

    pub fn place_terminal(&mut self, id: TerminalId, anchor: Option<TerminalId>) {
        if let Some(tabs) = self.tabs_mut() {
            match anchor {
                Some(anchor) => tabs.add_terminal_after(id, anchor),
                None => tabs.add_terminal(id),
            }
        }
        self.ensure_sessions();
        self.assign_terminals();
    }

    pub fn remove_terminal_tiles(&mut self, id: TerminalId) {
        for session in &mut self.sessions {
            session.tabs.remove_terminal(id);
        }
        self.drop_worker(id);
    }

    #[must_use]
    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|session| session.id == id)
    }

    #[must_use]
    pub fn current_session(&self) -> Option<&Session> {
        self.session(self.active_session?)
    }

    pub fn current_session_mut(&mut self) -> Option<&mut Session> {
        let id = self.active_session?;
        self.sessions.iter_mut().find(|session| session.id == id)
    }

    pub fn sessions_of(&self, group: Option<GroupId>) -> impl Iterator<Item = &Session> {
        self.sessions
            .iter()
            .filter(move |session| session.group == group)
    }

    #[must_use]
    pub fn session_of(&self, terminal: TerminalId) -> Option<SessionId> {
        self.sessions
            .iter()
            .find(|session| session.tabs.tab_of(terminal).is_some())
            .map(|session| session.id)
    }

    pub fn select_session(&mut self, id: SessionId) -> bool {
        let exists = self.session(id).is_some();
        if exists {
            self.active_session = Some(id);
        }
        exists
    }

    #[must_use]
    pub fn can_close_session(&self, id: SessionId) -> bool {
        let Some(session) = self.session(id) else {
            return false;
        };
        let empty = session.tabs.tabs.iter().all(|tab| tab.tiles.is_empty());
        empty || self.sessions_of(session.group).count() > 1
    }

    pub fn close_session(&mut self, id: SessionId) -> bool {
        let Some(group) = self.session(id).map(|session| session.group) else {
            return false;
        };
        if !self.can_close_session(id) {
            return false;
        }
        self.sessions.retain(|session| session.id != id);
        if self.active_session == Some(id) {
            self.active_session = self.heir_session(group);
        }
        self.assign_terminals();
        true
    }

    pub fn reveal_terminal(&mut self, terminal: TerminalId) {
        let Some(session) = self.session_of(terminal) else {
            return;
        };
        self.active_session = Some(session);
        if let Some(tabs) = self.tabs_mut() {
            if let Some(tab) = tabs.tab_of(terminal) {
                tabs.select(tab);
            }
        }
    }

    pub(super) fn push_session(&mut self, group: Option<GroupId>, name: String) -> SessionId {
        let id = SessionId(
            self.sessions
                .iter()
                .map(|session| session.id.as_u64())
                .max()
                .map_or(1, |last| last + 1),
        );
        self.sessions.push(Session {
            id,
            group,
            name,
            tabs: MosaicState::with_terminals(&[]),
            lead: None,
            panes: SessionPanes::default(),
            look: SessionLook::default(),
        });
        id
    }

    pub(super) fn home_of(&self, node: &TerminalNode) -> Option<GroupId> {
        node.group.filter(|group| self.group(*group).is_some())
    }

    pub(super) fn heir_session(&self, group: Option<GroupId>) -> Option<SessionId> {
        self.sessions_of(group)
            .next()
            .or_else(|| self.sessions.first())
            .map(|session| session.id)
    }

    fn ensure_sessions(&mut self) {
        let known: Vec<GroupId> = self.groups.iter().map(|group| group.id).collect();
        self.sessions
            .retain(|session| session.group.is_none_or(|group| known.contains(&group)));
        let mut homes: Vec<Option<GroupId>> = Vec::new();
        for node in &self.nodes {
            let home = self.home_of(node);
            if !homes.contains(&home) {
                homes.push(home);
            }
        }
        for home in homes {
            if self.sessions_of(home).next().is_none() {
                self.push_session(home, DEFAULT_SESSION_NAME.to_owned());
            }
        }
        if self.current_session().is_none() {
            self.active_session = self.sessions.first().map(|session| session.id);
        }
    }

    fn assign_terminals(&mut self) {
        let mut assigned: HashMap<SessionId, Vec<TerminalId>> = HashMap::new();
        for node in &self.nodes {
            let home = self.home_of(node);
            let holder = self
                .sessions_of(home)
                .find(|session| session.tabs.tab_of(node.id).is_some())
                .or_else(|| {
                    self.sessions_of(home)
                        .find(|session| Some(session.id) == self.active_session)
                })
                .or_else(|| self.sessions_of(home).next());
            if let Some(holder) = holder {
                assigned.entry(holder.id).or_default().push(node.id);
            }
        }
        for session in &mut self.sessions {
            let ids = assigned.get(&session.id).map_or(&[][..], Vec::as_slice);
            session.tabs.reconcile(ids);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::seed_environment;
    use crate::group::SessionSpec;
    use std::path::PathBuf;

    fn holders(environment: &EnvironmentData, id: TerminalId) -> usize {
        environment
            .sessions
            .iter()
            .filter(|session| session.tabs.tab_of(id).is_some())
            .count()
    }

    fn grouped_environment() -> (EnvironmentData, GroupId, GroupId) {
        let mut environment = seed_environment();
        let api = environment.add_group(PathBuf::from("/work/api"), None);
        let web = environment.add_group(PathBuf::from("/work/web"), None);
        environment.nodes[0].group = Some(api);
        environment.nodes[1].group = Some(api);
        (environment, api, web)
    }

    fn session_terminals(environment: &EnvironmentData, group: Option<GroupId>) -> Vec<TerminalId> {
        environment
            .sessions_of(group)
            .flat_map(|session| session.tabs.tabs.iter())
            .flat_map(|tab| tab.tiles.iter().map(|tile| tile.id))
            .collect()
    }

    #[test]
    fn sessions_mode_gives_a_default_session_only_to_groups_with_terminals() {
        let (mut environment, api, web) = grouped_environment();
        environment.enter_sessions();
        assert_eq!(environment.sessions.len(), 2, "api and the ungrouped");
        assert_eq!(environment.sessions_of(Some(web)).count(), 0);
        assert!(environment
            .sessions
            .iter()
            .all(|session| session.name == DEFAULT_SESSION_NAME));
        assert_eq!(
            session_terminals(&environment, Some(api)),
            [TerminalId(1), TerminalId(2)]
        );
        assert_eq!(session_terminals(&environment, Some(web)), Vec::new());
        assert_eq!(session_terminals(&environment, None), [TerminalId(3)]);
        environment.enter_sessions();
        assert_eq!(environment.sessions.len(), 2, "entering twice is a no-op");
    }

    #[test]
    fn closing_a_session_hands_its_terminals_to_the_group() {
        let (mut environment, api, _) = grouped_environment();
        environment.enter_sessions();
        let second = environment.new_session(Some(api), SessionSpec::default());
        environment.reveal_terminal(TerminalId(1));
        let main = environment.active_session;
        assert_ne!(main, Some(second));
        environment.active_session = Some(second);
        environment.remove_terminal_tiles(TerminalId(2));
        environment.place_terminal(TerminalId(2), None);
        assert_eq!(environment.session_of(TerminalId(2)), Some(second));
        assert!(environment.close_session(second));
        assert_eq!(environment.session_of(TerminalId(2)), main);
        assert_eq!(holders(&environment, TerminalId(2)), 1);
        let Some(main) = main else {
            return;
        };
        assert!(
            !environment.close_session(main),
            "its terminals need another session to move to"
        );
    }

    #[test]
    fn an_empty_session_closes_even_when_it_is_the_last_one() {
        let (mut environment, _, web) = grouped_environment();
        environment.enter_sessions();
        let only = environment.new_session(Some(web), SessionSpec::default());
        assert!(environment.close_session(only));
        assert_eq!(environment.sessions_of(Some(web)).count(), 0);
        assert!(environment.current_session().is_some());
    }

    #[test]
    fn removing_a_group_in_sessions_mode_removes_its_sessions() {
        let (mut environment, api, _) = grouped_environment();
        environment.enter_sessions();
        environment.new_session(Some(api), SessionSpec::default());
        assert!(environment.remove_group(api).is_some());
        assert!(environment
            .sessions
            .iter()
            .all(|session| session.group != Some(api)));
        assert!(environment.current_session().is_some(), "another one shows");
        for id in [TerminalId(1), TerminalId(2), TerminalId(3)] {
            assert_eq!(holders(&environment, id), 1, "{id:?}");
        }
        let added = environment.add_group(PathBuf::from("/work/docs"), None);
        assert_eq!(
            environment.sessions_of(Some(added)).count(),
            0,
            "a new group starts empty"
        );
    }
}
