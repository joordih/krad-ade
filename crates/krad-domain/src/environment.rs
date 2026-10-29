use crate::agent::{AgentPreset, AgentState};
use crate::group::{Session, TerminalGroup};
use crate::ids::{EnvironmentName, ResumeId, SessionId, TerminalId};
use crate::side_panel::SidePanelLayout;
use crate::text_scale::TextScale;
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
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    #[default]
    Terminal,
    Chat,
    File,
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
    pub nodes: Vec<TerminalNode>,
    pub meta: HashMap<TerminalId, TerminalMeta>,
    pub next_id: u64,
    #[serde(default)]
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
}
