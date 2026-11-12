use crate::agent::AgentState;
use crate::environment::{EnvironmentData, NodeKind, TerminalNode};
use crate::ids::{GroupId, TerminalId};
use crate::text_scale::TextScale;
use crate::view::EnvironmentView;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExtensionTab {
    pub extension: String,
    pub view: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShownExtensionViews {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar: Option<ExtensionTab>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_panel: Option<ExtensionTab>,
}

impl ShownExtensionViews {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.sidebar.is_none() && self.side_panel.is_none()
    }
}

impl EnvironmentData {
    #[must_use]
    pub fn extension_tab_of(&self, id: TerminalId) -> Option<&ExtensionTab> {
        self.nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.extension.as_ref())
    }

    pub fn open_extension_tab(
        &mut self,
        group: Option<GroupId>,
        tab: ExtensionTab,
        title: &str,
    ) -> TerminalId {
        let session = self.session_for(group);
        self.active_session = Some(session);
        self.view = EnvironmentView::Sessions;
        if let Some(id) = self
            .nodes
            .iter()
            .find(|node| node.extension.as_ref() == Some(&tab))
            .map(|node| node.id)
        {
            self.reveal_terminal(id);
            return id;
        }
        let id = TerminalId(self.next_id);
        self.next_id += 1;
        self.nodes.push(TerminalNode {
            id,
            title: title.to_owned(),
            state: AgentState::Idle,
            harness: None,
            resume: None,
            group,
            text_scale: TextScale::default(),
            kind: NodeKind::Extension,
            file: None,
            extension: Some(tab),
            bookmarks: crate::bookmark::ChatBookmarks::default(),
            usage: crate::usage::ChatUsage::default(),
            checkpoints: crate::checkpoint::ChatCheckpoints::default(),
            choice: crate::model::ChatChoice::default(),
            cloud: None,
        });
        self.place_terminal(id, None);
        self.reveal_terminal(id);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::EnvironmentName;

    fn containers() -> ExtensionTab {
        ExtensionTab {
            extension: "docker".to_owned(),
            view: "container".to_owned(),
            instance: Some("web".to_owned()),
        }
    }

    #[test]
    fn opening_the_same_extension_view_twice_reveals_one_tab() {
        let mut env = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
        let first = env.open_extension_tab(None, containers(), "web");
        let again = env.open_extension_tab(None, containers(), "web");
        assert_eq!(first, again);
        assert_eq!(env.extension_tab_of(first), Some(&containers()));
        let other = ExtensionTab {
            instance: Some("db".to_owned()),
            ..containers()
        };
        assert_ne!(env.open_extension_tab(None, other, "db"), first);
    }

    #[test]
    fn an_extension_tab_persists_with_its_kind_and_view() -> serde_json::Result<()> {
        let mut env = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
        let id = env.open_extension_tab(None, containers(), "web");
        let text = serde_json::to_string(&env)?;
        let back: EnvironmentData = serde_json::from_str(&text)?;
        let node = back.nodes.iter().find(|node| node.id == id);
        assert_eq!(node.map(|node| node.kind), Some(NodeKind::Extension));
        assert_eq!(back.extension_tab_of(id), Some(&containers()));
        Ok(())
    }

    #[test]
    fn the_shown_sidebar_view_persists_and_an_empty_one_is_not_written() -> serde_json::Result<()> {
        let mut env = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
        let quiet = serde_json::to_value(&env)?;
        assert!(quiet.get("extension_views").is_none());
        env.extension_views.sidebar = Some(containers());
        let text = serde_json::to_string(&env)?;
        let back: EnvironmentData = serde_json::from_str(&text)?;
        assert_eq!(back.extension_views.sidebar, Some(containers()));
        assert_eq!(back.extension_views.side_panel, None);
        Ok(())
    }
}
