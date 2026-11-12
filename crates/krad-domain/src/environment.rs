use crate::agent::{AgentPreset, AgentState};
use crate::archive::Archive;
use crate::bookmark::ChatBookmarks;
use crate::checkpoint::ChatCheckpoints;
use crate::group::{Session, ShownExtensionViews, TerminalGroup};
use crate::host::Host;
use crate::ids::{EnvironmentName, ResumeId, SessionId, TerminalId};
use crate::model::ChatChoice;
use crate::side_panel::SidePanelLayout;
use crate::text_scale::TextScale;
use crate::usage::{ChatUsage, RetiredSpend};
use crate::variants::VariantRuns;
use crate::view::EnvironmentView;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalNode {
    pub id: TerminalId,
    pub title: String,
    pub state: AgentState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<AgentPreset>,
    #[serde(
        default,
        deserialize_with = "valid_resume_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub resume: Option<ResumeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<crate::ids::GroupId>,
    #[serde(default)]
    pub text_scale: TextScale,
    #[serde(default, skip_serializing_if = "NodeKind::is_terminal")]
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<crate::group::OpenFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<crate::group::ExtensionTab>,
    #[serde(default, skip_serializing_if = "ChatBookmarks::is_empty")]
    pub bookmarks: ChatBookmarks,
    #[serde(default, skip_serializing_if = "ChatUsage::is_empty")]
    pub usage: ChatUsage,
    #[serde(default, skip_serializing_if = "ChatCheckpoints::is_empty")]
    pub checkpoints: ChatCheckpoints,
    #[serde(default, skip_serializing_if = "ChatChoice::is_empty")]
    pub choice: ChatChoice,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    #[default]
    Terminal,
    Chat,
    File,
    Extension,
}

impl NodeKind {
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminal)
    }
}

fn valid_resume_id<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ResumeId>, D::Error> {
    let raw = Option::<String>::deserialize(deserializer)?;
    Ok(raw.and_then(|raw| ResumeId::try_from(raw).ok()))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalMeta {
    pub cwd: String,
    pub scrollback: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvironmentData {
    pub name: EnvironmentName,
    #[serde(default, skip_serializing_if = "Host::is_local")]
    pub host: Host,
    pub nodes: Vec<TerminalNode>,
    pub meta: HashMap<TerminalId, TerminalMeta>,
    pub next_id: u64,
    #[serde(default, deserialize_with = "crate::view::lenient_view")]
    pub view: EnvironmentView,
    #[serde(default)]
    pub focus: Option<TerminalId>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub groups: Vec<TerminalGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<Session>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_session: Option<SessionId>,
    #[serde(default)]
    pub side_panel: SidePanelLayout,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub note_tabs: Vec<crate::notes::NoteTabs>,
    #[serde(default, skip_serializing_if = "ShownExtensionViews::is_empty")]
    pub extension_views: ShownExtensionViews,
    #[serde(default, skip_serializing_if = "RetiredSpend::is_empty")]
    pub retired_spend: RetiredSpend,
    #[serde(default, skip_serializing_if = "VariantRuns::is_empty")]
    pub variant_runs: VariantRuns,
    #[serde(default, skip_serializing_if = "Archive::is_empty")]
    pub archive: Archive,
}

impl EnvironmentData {
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.title.as_deref().unwrap_or(self.name.as_str())
    }
}

pub const DEFAULT_ENVIRONMENT: &str = "main";

impl EnvironmentData {
    #[must_use]
    pub fn empty(name: EnvironmentName) -> Self {
        Self {
            name,
            host: Host::Local,
            nodes: Vec::new(),
            view: EnvironmentView::default(),
            focus: None,
            meta: HashMap::new(),
            next_id: 1,
            title: None,
            groups: Vec::new(),
            sessions: Vec::new(),
            active_session: None,
            side_panel: SidePanelLayout::default(),
            note_tabs: Vec::new(),
            extension_views: ShownExtensionViews::default(),
            retired_spend: RetiredSpend::default(),
            variant_runs: VariantRuns::default(),
            archive: Archive::default(),
        }
    }
}

#[cfg(test)]
const SEED_NODES: [(u64, &str, AgentState); 3] = [
    (1, "opencode · auth", AgentState::Working),
    (2, "claude · payments", AgentState::WaitingForInput),
    (3, "shell · logs", AgentState::Idle),
];

#[cfg(test)]
const SEED_NEXT_ID: u64 = 4;

#[cfg(test)]
pub(crate) fn seed_environment() -> EnvironmentData {
    let (terminals, next_id) = seed_terminals();
    EnvironmentData {
        nodes: terminals,
        next_id,
        ..EnvironmentData::empty(EnvironmentName::new(DEFAULT_ENVIRONMENT.to_owned()))
    }
}

#[cfg(test)]
pub(crate) fn seed_terminals() -> (Vec<TerminalNode>, u64) {
    let terminals = SEED_NODES
        .iter()
        .map(|&(id, title, state)| TerminalNode {
            id: TerminalId(id),
            title: title.to_owned(),
            state,
            harness: None,
            resume: None,
            group: None,
            text_scale: TextScale::default(),
            kind: NodeKind::default(),
            file: None,
            extension: None,
            bookmarks: ChatBookmarks::default(),
            usage: ChatUsage::default(),
            checkpoints: ChatCheckpoints::default(),
            choice: ChatChoice::default(),
        })
        .collect();
    (terminals, SEED_NEXT_ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_prefers_the_name_the_user_gave() {
        let mut environment = seed_environment();
        assert_eq!(environment.display_name(), DEFAULT_ENVIRONMENT);
        environment.title = Some("Payments".to_owned());
        assert_eq!(environment.display_name(), "Payments");
        assert_eq!(environment.name.as_str(), DEFAULT_ENVIRONMENT);
    }

    #[test]
    fn a_node_written_before_bookmarks_loads_without_any() -> serde_json::Result<()> {
        let older = serde_json::json!({
            "id": 7,
            "title": "Chat",
            "state": "Idle",
            "kind": "Chat"
        });
        let node: TerminalNode = serde_json::from_value(older)?;
        assert!(node.bookmarks.is_empty());
        assert!(serde_json::to_value(&node)?.get("bookmarks").is_none());
        Ok(())
    }

    #[test]
    fn a_chat_keeps_its_model_and_effort_and_older_nodes_have_none() -> serde_json::Result<()> {
        let older: TerminalNode = serde_json::from_value(serde_json::json!({
            "id": 3, "title": "Chat", "state": "Idle", "kind": "Chat"
        }))?;
        assert!(older.choice.is_empty());
        assert!(serde_json::to_value(&older)?.get("choice").is_none());
        let mut node = older;
        node.choice = ChatChoice {
            model: crate::model::ModelId::parse("claude-fable-5-1[1m]"),
            effort: Some(crate::model::Effort::Max),
        };
        let back: TerminalNode = serde_json::from_value(serde_json::to_value(&node)?)?;
        assert_eq!(back.choice, node.choice);
        Ok(())
    }

    #[test]
    fn a_chat_keeps_its_bookmarks_across_a_restart() -> serde_json::Result<()> {
        let (mut nodes, _) = seed_terminals();
        let mut node = nodes.remove(0);
        node.bookmarks.toggle(crate::Bookmark {
            turn: crate::TurnPrint::from("Fix the build"),
            repeat: 1,
            side: crate::TurnSide::Answer,
        });
        let back: TerminalNode = serde_json::from_str(&serde_json::to_string(&node)?)?;
        assert_eq!(back.bookmarks, node.bookmarks);
        Ok(())
    }
}
