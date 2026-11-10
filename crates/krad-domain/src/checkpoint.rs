use serde::{Deserialize, Serialize};

use crate::agent::AgentPreset;
use crate::git_object::GitObjectId;
use crate::rewind::RewindPoint;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnCheckpoint {
    pub point: RewindPoint,
    pub checkpoint: GitObjectId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChatCheckpoints(Vec<TurnCheckpoint>);

impl ChatCheckpoints {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn before(&self, point: &RewindPoint) -> Option<&GitObjectId> {
        self.0
            .iter()
            .find(|turn| &turn.point == point)
            .map(|turn| &turn.checkpoint)
    }

    pub fn record(&mut self, point: RewindPoint, checkpoint: GitObjectId) {
        self.0.retain(|turn| turn.point != point);
        self.0.push(TurnCheckpoint { point, checkpoint });
    }

    pub fn cut_from(&mut self, point: &RewindPoint) {
        if let Some(index) = self.0.iter().position(|turn| &turn.point == point) {
            self.0.truncate(index);
        }
    }
}

impl AgentPreset {
    #[must_use]
    pub const fn checkpoints_files(self) -> bool {
        self.rewinds() && !self.restores_files()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::built_in_acp_agents;
    use crate::git_object::ObjectIdError;
    use crate::rewind::RewindId;

    const TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
    const OTHER_TREE: &str = "0123456789abcdef0123456789abcdef01234567";

    fn turn(id: &str) -> RewindPoint {
        RewindPoint::Turn(RewindId::from(id))
    }

    #[test]
    fn only_harnesses_that_rewind_without_their_files_take_checkpoints() {
        for preset in AgentPreset::listed(&built_in_acp_agents()) {
            let expected = matches!(preset, AgentPreset::Codex);
            assert_eq!(preset.checkpoints_files(), expected, "{preset:?}");
        }
    }

    #[test]
    fn a_checkpoint_is_a_git_object_id() {
        assert_eq!(
            TREE.parse::<GitObjectId>().map(String::from),
            Ok(TREE.to_owned())
        );
        assert_eq!(
            format!(" {TREE}\n").parse::<GitObjectId>().ok(),
            TREE.parse().ok()
        );
        for invalid in ["", "HEAD", "4b825dc", &TREE.to_uppercase()] {
            assert_eq!(
                invalid.parse::<GitObjectId>(),
                Err(ObjectIdError),
                "{invalid}"
            );
        }
    }

    #[test]
    fn checkpoints_are_found_by_turn_replaced_and_cut_with_the_conversation(
    ) -> Result<(), ObjectIdError> {
        let tree: GitObjectId = TREE.parse()?;
        let other: GitObjectId = OTHER_TREE.parse()?;
        let mut checkpoints = ChatCheckpoints::default();
        checkpoints.record(turn("1"), tree.clone());
        checkpoints.record(turn("2"), tree.clone());
        checkpoints.record(turn("3"), tree.clone());
        checkpoints.record(turn("2"), other.clone());
        assert_eq!(checkpoints.before(&turn("2")), Some(&other));
        assert_eq!(checkpoints.before(&turn("4")), None);

        checkpoints.cut_from(&turn("3"));
        assert_eq!(checkpoints.before(&turn("3")), None);
        assert_eq!(checkpoints.before(&turn("1")), Some(&tree));
        checkpoints.cut_from(&turn("1"));
        assert!(checkpoints.is_empty());
        Ok(())
    }

    #[test]
    fn checkpoints_persist_as_a_list_and_reject_a_bad_id() -> Result<(), ObjectIdError> {
        let mut checkpoints = ChatCheckpoints::default();
        checkpoints.record(turn("t1"), TREE.parse()?);
        let persisted = format!(r#"[{{"point":{{"Turn":"t1"}},"checkpoint":"{TREE}"}}]"#);
        assert_eq!(
            serde_json::to_string(&checkpoints).ok().as_deref(),
            Some(persisted.as_str())
        );
        assert_eq!(
            serde_json::from_str::<ChatCheckpoints>(&persisted).ok(),
            Some(checkpoints)
        );
        let bad = r#"[{"point":{"Turn":"t1"},"checkpoint":"HEAD"}]"#;
        assert!(serde_json::from_str::<ChatCheckpoints>(bad).is_err());
        Ok(())
    }

    #[test]
    fn a_chat_saved_before_checkpoints_loads_without_any_and_saves_them_once_taken(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let saved = r#"{"id":4,"title":"Chat","state":"Idle","kind":"Chat"}"#;
        let mut node: crate::environment::TerminalNode = serde_json::from_str(saved)?;
        assert!(node.checkpoints.is_empty());
        assert!(!serde_json::to_string(&node)?.contains("checkpoints"));
        node.checkpoints.record(turn("t1"), TREE.parse()?);
        let line = serde_json::to_string(&node)?;
        let loaded: crate::environment::TerminalNode = serde_json::from_str(&line)?;
        assert_eq!(loaded.checkpoints, node.checkpoints);
        Ok(())
    }
}
