use crate::environment::{EnvironmentData, NodeKind};
use crate::ids::TerminalId;

impl EnvironmentData {
    fn is_chat(&self, id: TerminalId) -> bool {
        self.nodes
            .iter()
            .any(|node| node.id == id && node.kind == NodeKind::Chat)
    }

    #[must_use]
    pub fn is_file_tab(&self, id: TerminalId) -> bool {
        self.nodes
            .iter()
            .any(|node| node.id == id && node.kind == NodeKind::File)
    }

    #[must_use]
    pub fn session_chats(&self) -> Vec<TerminalId> {
        let Some(session) = self.current_session() else {
            return Vec::new();
        };
        session
            .tabs
            .tabs
            .iter()
            .flat_map(|tab| tab.tiles.iter().map(|tile| tile.id))
            .filter(|id| self.is_chat(*id))
            .collect()
    }

    #[must_use]
    pub fn editor_chat(&self) -> Option<TerminalId> {
        let chats = self.session_chats();
        let last = self.current_session().and_then(|session| session.last_chat);
        last.filter(|id| chats.contains(id))
            .or_else(|| chats.first().copied())
    }

    pub fn remember_chat(&mut self, id: TerminalId) {
        if !self.is_chat(id) {
            return;
        }
        if let Some(session) = self.current_session_mut() {
            session.last_chat = Some(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::environment::{EnvironmentData, NodeKind, TerminalNode};
    use crate::group::{FileOpening, TerminalGroup};
    use crate::ids::{EnvironmentName, GroupId, TerminalId};
    use crate::{AgentState, TextScale};
    use std::path::{Path, PathBuf};

    const GROUP: GroupId = GroupId(1);

    fn sessions_env() -> EnvironmentData {
        let mut env = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
        env.groups.push(TerminalGroup {
            id: GROUP,
            name: "api".to_owned(),
            root: PathBuf::from("/w/api"),
            worktree: None,
        });
        env.enter_sessions();
        env
    }

    fn add_chat(env: &mut EnvironmentData) -> TerminalId {
        let id = TerminalId(env.next_id);
        env.next_id += 1;
        env.nodes.push(TerminalNode {
            id,
            title: "Chat".to_owned(),
            state: AgentState::Idle,
            harness: None,
            resume: None,
            group: Some(GROUP),
            text_scale: TextScale::default(),
            kind: NodeKind::Chat,
            file: None,
        });
        env.place_terminal(id, None);
        id
    }

    #[test]
    fn the_editor_shows_the_chat_last_focused_in_the_session() {
        let mut env = sessions_env();
        let file = env.open_file(
            Some(GROUP),
            Path::new("/w/api/main.rs"),
            FileOpening::Pinned,
        );
        let first = add_chat(&mut env);
        let second = add_chat(&mut env);
        assert_eq!(
            env.editor_chat(),
            Some(first),
            "the first chat until one is focused"
        );
        env.reveal_terminal(second);
        env.reveal_terminal(file);
        assert_eq!(
            env.editor_chat(),
            Some(second),
            "a file tab does not take its place"
        );
        assert_eq!(env.session_chats(), [first, second]);
    }

    #[test]
    fn only_file_nodes_are_file_tabs() {
        let mut env = sessions_env();
        let file = env.open_file(Some(GROUP), Path::new("/w/api/lib.rs"), FileOpening::Pinned);
        let chat = add_chat(&mut env);
        assert!(env.is_file_tab(file));
        assert!(!env.is_file_tab(chat));
        assert_eq!(env.editor_chat(), Some(chat));
    }
}
