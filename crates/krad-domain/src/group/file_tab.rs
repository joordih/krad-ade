use super::DEFAULT_SESSION_NAME;
use crate::agent::AgentState;
use crate::environment::{EnvironmentData, NodeKind, TerminalNode};
use crate::gateway::Route;
use crate::ids::{GroupId, SessionId, TerminalId};
use crate::text_scale::TextScale;
use crate::view::EnvironmentView;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileView {
    #[default]
    Edit,
    Changes,
}

impl FileView {
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's skip_serializing_if hands over a reference"
    )]
    const fn is_edit(&self) -> bool {
        matches!(self, Self::Edit)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenFile {
    pub path: PathBuf,
    pub pinned: bool,
    #[serde(default, skip_serializing_if = "FileView::is_edit")]
    pub view: FileView,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOpening {
    Preview,
    Pinned,
}

const CHANGES_TITLE_SUFFIX: &str = " (changes)";

fn file_title(path: &Path, view: FileView) -> String {
    let name = path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    match view {
        FileView::Edit => name,
        FileView::Changes => format!("{name}{CHANGES_TITLE_SUFFIX}"),
    }
}

impl EnvironmentData {
    #[must_use]
    pub fn open_file_of(&self, id: TerminalId) -> Option<&OpenFile> {
        self.nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.file.as_ref())
    }

    pub fn open_file(
        &mut self,
        group: Option<GroupId>,
        path: &Path,
        opening: FileOpening,
    ) -> TerminalId {
        self.open_file_as(group, path, FileView::Edit, opening)
    }

    pub fn open_file_as(
        &mut self,
        group: Option<GroupId>,
        path: &Path,
        view: FileView,
        opening: FileOpening,
    ) -> TerminalId {
        let session = self.session_for(group);
        self.active_session = Some(session);
        self.view = EnvironmentView::Sessions;
        let pinned = opening == FileOpening::Pinned;
        let tabs = self.file_tabs_of(session);
        if let Some(id) = tabs.iter().copied().find(|id| {
            self.open_file_of(*id)
                .is_some_and(|file| file.path == path && file.view == view)
        }) {
            if pinned {
                self.pin_file(id);
            }
            self.reveal_terminal(id);
            return id;
        }
        let preview = tabs
            .iter()
            .copied()
            .find(|id| self.open_file_of(*id).is_some_and(|file| !file.pinned));
        if let (Some(id), false) = (preview, pinned) {
            if let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) {
                node.title = file_title(path, view);
                node.file = Some(OpenFile {
                    path: path.to_path_buf(),
                    pinned: false,
                    view,
                });
            }
            self.reveal_terminal(id);
            return id;
        }
        let id = TerminalId(self.next_id);
        self.next_id += 1;
        self.nodes.push(TerminalNode {
            id,
            title: file_title(path, view),
            state: AgentState::Idle,
            harness: None,
            resume: None,
            group,
            text_scale: TextScale::default(),
            kind: NodeKind::File,
            file: Some(OpenFile {
                path: path.to_path_buf(),
                pinned,
                view,
            }),
            route: Route::Direct,
        });
        self.place_terminal(id, None);
        self.reveal_terminal(id);
        id
    }

    pub fn pin_file(&mut self, id: TerminalId) {
        if let Some(file) = self
            .nodes
            .iter_mut()
            .find(|node| node.id == id)
            .and_then(|node| node.file.as_mut())
        {
            file.pinned = true;
        }
    }

    pub fn drop_file_tabs(&mut self, keep_pinned: bool) {
        let dropped: Vec<TerminalId> = self
            .nodes
            .iter()
            .filter(|node| {
                node.file
                    .as_ref()
                    .is_some_and(|file| !(keep_pinned && file.pinned))
            })
            .map(|node| node.id)
            .collect();
        for id in dropped {
            self.nodes.retain(|node| node.id != id);
            self.remove_terminal_tiles(id);
        }
    }

    fn file_tabs_of(&self, session: SessionId) -> Vec<TerminalId> {
        let Some(session) = self.session(session) else {
            return Vec::new();
        };
        session
            .tabs
            .tabs
            .iter()
            .flat_map(|tab| tab.tiles.iter().map(|tile| tile.id))
            .filter(|id| self.open_file_of(*id).is_some())
            .collect()
    }

    fn session_for(&mut self, group: Option<GroupId>) -> SessionId {
        let on_screen = self
            .current_session()
            .filter(|session| session.group == group)
            .map(|session| session.id);
        on_screen
            .or_else(|| self.sessions_of(group).next().map(|session| session.id))
            .unwrap_or_else(|| self.push_session(group, DEFAULT_SESSION_NAME.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::TerminalGroup;
    use crate::ids::EnvironmentName;

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

    fn open_paths(env: &EnvironmentData) -> Vec<(String, bool)> {
        env.nodes
            .iter()
            .filter_map(|node| node.file.as_ref())
            .map(|file| (file.path.to_string_lossy().into_owned(), file.pinned))
            .collect()
    }

    #[test]
    fn a_file_opens_in_a_default_session_of_a_group_without_one() {
        let mut env = sessions_env();
        env.view = EnvironmentView::Notes;
        let id = env.open_file(
            Some(GROUP),
            Path::new("/w/api/lib.rs"),
            FileOpening::Preview,
        );
        assert_eq!(env.sessions_of(Some(GROUP)).count(), 1);
        assert_eq!(env.session_of(id), env.active_session);
        assert_eq!(env.view, EnvironmentView::Sessions);
    }

    #[test]
    fn one_click_replaces_the_preview_tab_and_a_double_click_pins() {
        let mut env = sessions_env();
        let first = env.open_file(Some(GROUP), Path::new("/w/api/a.rs"), FileOpening::Preview);
        let second = env.open_file(Some(GROUP), Path::new("/w/api/b.rs"), FileOpening::Preview);
        assert_eq!(first, second);
        assert_eq!(open_paths(&env), [("/w/api/b.rs".to_owned(), false)]);
        env.open_file(Some(GROUP), Path::new("/w/api/b.rs"), FileOpening::Pinned);
        env.open_file(Some(GROUP), Path::new("/w/api/c.rs"), FileOpening::Preview);
        assert_eq!(
            open_paths(&env),
            [
                ("/w/api/b.rs".to_owned(), true),
                ("/w/api/c.rs".to_owned(), false)
            ]
        );
    }

    #[test]
    fn opening_an_open_file_shows_its_tab() {
        let mut env = sessions_env();
        let pinned = env.open_file(Some(GROUP), Path::new("/w/api/a.rs"), FileOpening::Pinned);
        env.open_file(Some(GROUP), Path::new("/w/api/b.rs"), FileOpening::Preview);
        let again = env.open_file(Some(GROUP), Path::new("/w/api/a.rs"), FileOpening::Preview);
        assert_eq!(again, pinned);
        assert_eq!(open_paths(&env).len(), 2);
    }

    #[test]
    fn a_file_shows_in_an_editor_tab_and_a_changes_tab_side_by_side() {
        let mut env = sessions_env();
        let path = Path::new("/w/api/a.rs");
        let edit = env.open_file(Some(GROUP), path, FileOpening::Pinned);
        let changes = env.open_file_as(Some(GROUP), path, FileView::Changes, FileOpening::Pinned);
        assert_ne!(edit, changes);
        assert_eq!(
            env.open_file_as(Some(GROUP), path, FileView::Changes, FileOpening::Preview),
            changes
        );
        let titles: Vec<&str> = env.nodes.iter().map(|node| node.title.as_str()).collect();
        assert_eq!(titles, ["a.rs", "a.rs (changes)"]);
    }

    #[test]
    fn an_editor_tab_saves_without_a_view() {
        let file = OpenFile {
            path: PathBuf::from("/w/a.rs"),
            pinned: true,
            view: FileView::Edit,
        };
        let saved = serde_json::to_string(&file).unwrap_or_default();
        assert!(!saved.contains("view"), "{saved}");
        let read: Result<OpenFile, _> = serde_json::from_str(&saved);
        assert!(read.is_ok_and(|read| read == file));
    }

    #[test]
    fn preview_tabs_close_on_reload() {
        let mut env = sessions_env();
        env.open_file(Some(GROUP), Path::new("/w/api/a.rs"), FileOpening::Pinned);
        env.open_file(Some(GROUP), Path::new("/w/api/b.rs"), FileOpening::Preview);
        env.drop_file_tabs(true);
        assert_eq!(open_paths(&env), [("/w/api/a.rs".to_owned(), true)]);
    }
}
