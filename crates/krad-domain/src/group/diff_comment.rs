mod review_message;

pub use review_message::review_message;

use super::{HunkFingerprint, Session};
use crate::environment::EnvironmentData;
use crate::ids::{SessionId, TerminalId};
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiffCommentId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawHunkLines")]
pub struct HunkLines {
    first: usize,
    last: usize,
}

#[derive(Deserialize)]
struct RawHunkLines {
    first: usize,
    last: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvertedHunkLines;

impl std::fmt::Display for InvertedHunkLines {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the first line of a hunk range comes after its last")
    }
}

impl std::error::Error for InvertedHunkLines {}

impl TryFrom<RawHunkLines> for HunkLines {
    type Error = InvertedHunkLines;

    fn try_from(raw: RawHunkLines) -> Result<Self, InvertedHunkLines> {
        if raw.first > raw.last {
            return Err(InvertedHunkLines);
        }
        Ok(Self {
            first: raw.first,
            last: raw.last,
        })
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LoadedComment {
    Valid(DiffComment),
    Invalid(serde::de::IgnoredAny),
}

pub(super) fn kept_diff_comments<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<DiffComment>, D::Error> {
    let loaded = Vec::<LoadedComment>::deserialize(deserializer)?;
    Ok(loaded
        .into_iter()
        .filter_map(|comment| match comment {
            LoadedComment::Valid(comment) => Some(comment),
            LoadedComment::Invalid(_) => None,
        })
        .collect())
}

impl HunkLines {
    #[must_use]
    pub fn new(one: usize, other: usize) -> Self {
        Self {
            first: one.min(other),
            last: one.max(other),
        }
    }

    #[must_use]
    pub const fn range(self) -> RangeInclusive<usize> {
        self.first..=self.last
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DiffAnchor {
    pub path: PathBuf,
    pub hunk: HunkFingerprint,
    pub lines: HunkLines,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffSide {
    Old,
    New,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LineSpan {
    pub side: DiffSide,
    pub first: u32,
    pub last: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DiffQuote {
    pub anchor: DiffAnchor,
    pub span: LineSpan,
    pub excerpt: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorState {
    #[default]
    Current,
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffComment {
    pub id: DiffCommentId,
    pub quote: DiffQuote,
    pub text: String,
    #[serde(default)]
    pub state: AnchorState,
}

impl Session {
    pub fn add_diff_comment(&mut self, quote: DiffQuote, text: String) -> DiffCommentId {
        let id = DiffCommentId(
            self.diff_comments
                .iter()
                .map(|comment| comment.id.0)
                .max()
                .map_or(1, |last| last + 1),
        );
        self.diff_comments.push(DiffComment {
            id,
            quote,
            text,
            state: AnchorState::Current,
        });
        id
    }

    pub fn edit_diff_comment(&mut self, id: DiffCommentId, text: String) -> bool {
        let Some(comment) = self
            .diff_comments
            .iter_mut()
            .find(|comment| comment.id == id)
        else {
            return false;
        };
        comment.text = text;
        true
    }

    pub fn remove_diff_comment(&mut self, id: DiffCommentId) -> bool {
        let before = self.diff_comments.len();
        self.diff_comments.retain(|comment| comment.id != id);
        self.diff_comments.len() != before
    }

    pub fn diff_comments_on<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Iterator<Item = &'a DiffComment> + 'a {
        self.diff_comments
            .iter()
            .filter(move |comment| comment.quote.anchor.path == path)
    }
}

impl EnvironmentData {
    #[must_use]
    pub fn review_recipient(&self, id: SessionId) -> Option<TerminalId> {
        let session = self.session(id)?;
        let chats: Vec<TerminalId> = session
            .tabs
            .tabs
            .iter()
            .flat_map(|tab| tab.tiles.iter().map(|tile| tile.id))
            .filter(|terminal| self.is_chat(*terminal))
            .collect();
        let lead = session.lead.as_ref().and_then(|lead| lead.terminal);
        lead.or(session.last_chat)
            .filter(|terminal| chats.contains(terminal))
            .or_else(|| chats.first().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{EnvironmentData, NodeKind, TerminalNode};
    use crate::group::{SessionLead, TerminalGroup, WorkerSpawn};
    use crate::ids::{EnvironmentName, GroupId};
    use crate::{AgentPreset, AgentState, TextScale};

    const GROUP: GroupId = GroupId(1);

    fn quote(path: &str, lines: HunkLines) -> DiffQuote {
        DiffQuote {
            anchor: DiffAnchor {
                path: PathBuf::from(path),
                hunk: [" a", "-b", "+c"].into_iter().collect(),
                lines,
            },
            span: LineSpan {
                side: DiffSide::New,
                first: 2,
                last: 2,
            },
            excerpt: vec!["+c".to_owned()],
        }
    }

    fn sessions_env() -> EnvironmentData {
        let mut env = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
        env.groups.push(TerminalGroup {
            id: GROUP,
            name: "api".to_owned(),
            root: PathBuf::from("/w/api"),
            worktree: None,
            activation: crate::toolchain::GroupActivation::Inherit,
        });
        env.enter_sessions();
        add_node(&mut env, NodeKind::Terminal);
        env
    }

    fn add_node(env: &mut EnvironmentData, kind: NodeKind) -> TerminalId {
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
            kind,
            file: None,
            extension: None,
            bookmarks: crate::bookmark::ChatBookmarks::default(),
            checkpoints: crate::checkpoint::ChatCheckpoints::default(),
            choice: crate::model::ChatChoice::default(),
            usage: crate::usage::ChatUsage::default(),
            cloud: None,
        });
        env.place_terminal(id, None);
        id
    }

    fn current(env: &EnvironmentData) -> SessionId {
        env.active_session
            .unwrap_or_else(|| unreachable!("sessions mode opens a session"))
    }

    #[test]
    fn the_fingerprint_follows_the_hunk_content_only() {
        let hunk: HunkFingerprint = [" a", "-b", "+c"].into_iter().collect();
        let same: HunkFingerprint = [" a", "-b", "+c"].into_iter().collect();
        let edited: HunkFingerprint = [" a", "-b", "+d"].into_iter().collect();
        let joined: HunkFingerprint = [" a-b", "+c"].into_iter().collect();
        assert_eq!(hunk, same);
        assert_ne!(hunk, edited);
        assert_ne!(hunk, joined, "line boundaries are part of the content");
    }

    #[test]
    fn hunk_lines_order_their_ends() {
        assert_eq!(HunkLines::new(4, 1).range(), 1..=4);
        assert_eq!(HunkLines::new(2, 2).range(), 2..=2);
    }

    #[test]
    fn saved_hunk_lines_in_inverted_order_are_refused() -> serde_json::Result<()> {
        let kept: HunkLines = serde_json::from_str(r#"{"first":1,"last":3}"#)?;
        assert_eq!(kept.range(), 1..=3);
        assert!(serde_json::from_str::<HunkLines>(r#"{"first":3,"last":1}"#).is_err());
        Ok(())
    }

    #[test]
    fn a_session_drops_a_saved_comment_with_inverted_lines_and_keeps_the_rest(
    ) -> serde_json::Result<()> {
        let mut env = sessions_env();
        let Some(target) = env.current_session_mut() else {
            unreachable!("a session is open");
        };
        target.add_diff_comment(quote("/w/api/a.rs", HunkLines::new(1, 2)), "kept".into());
        target.add_diff_comment(quote("/w/api/b.rs", HunkLines::new(3, 4)), "bad".into());
        let saved = serde_json::to_string(&*target)?.replace(
            r#""lines":{"first":3,"last":4}"#,
            r#""lines":{"first":4,"last":3}"#,
        );
        let loaded: Session = serde_json::from_str(&saved)?;
        let texts: Vec<&str> = loaded
            .diff_comments
            .iter()
            .map(|comment| comment.text.as_str())
            .collect();
        assert_eq!(texts, ["kept"]);
        Ok(())
    }

    #[test]
    fn comments_are_added_edited_and_removed_per_session() {
        let mut env = sessions_env();
        let session = current(&env);
        let Some(target) = env.current_session_mut() else {
            unreachable!("a session is open");
        };
        let first = target.add_diff_comment(quote("/w/api/a.rs", HunkLines::new(2, 2)), "x".into());
        let second =
            target.add_diff_comment(quote("/w/api/b.rs", HunkLines::new(0, 1)), "y".into());
        assert_ne!(first, second);
        assert!(target.edit_diff_comment(first, "renamed".into()));
        assert_eq!(
            target
                .diff_comments_on(Path::new("/w/api/a.rs"))
                .map(|comment| comment.text.as_str())
                .collect::<Vec<_>>(),
            ["renamed"]
        );
        assert!(target.remove_diff_comment(second));
        assert!(!target.remove_diff_comment(second));
        assert_eq!(env.session(session).map(|s| s.diff_comments.len()), Some(1));
    }

    #[test]
    fn comments_roundtrip_through_the_layout_and_old_layouts_have_none() -> serde_json::Result<()> {
        let mut env = sessions_env();
        if let Some(session) = env.current_session_mut() {
            let id =
                session.add_diff_comment(quote("/w/api/a.rs", HunkLines::new(1, 3)), "why".into());
            if let Some(comment) = session.diff_comments.iter_mut().find(|c| c.id == id) {
                comment.state = AnchorState::Stale;
            }
        }
        let json = serde_json::to_string(&env.sessions)?;
        let back: Vec<Session> = serde_json::from_str(&json)?;
        assert_eq!(back, env.sessions);

        let mut bare = serde_json::to_value(&env.sessions)?;
        if let Some(session) = bare.get_mut(0).and_then(serde_json::Value::as_object_mut) {
            session.remove("diff_comments");
        }
        let old: Vec<Session> = serde_json::from_value(bare)?;
        assert_eq!(old[0].diff_comments, []);

        let mut stateless = serde_json::to_value(&env.sessions)?;
        if let Some(comment) = stateless
            .pointer_mut("/0/diff_comments/0")
            .and_then(serde_json::Value::as_object_mut)
        {
            comment.remove("state");
        }
        let current: Vec<Session> = serde_json::from_value(stateless)?;
        assert_eq!(current[0].diff_comments[0].state, AnchorState::Current);
        Ok(())
    }

    #[test]
    fn a_review_goes_to_the_lead_then_the_last_chat_then_the_first_chat() {
        let mut env = sessions_env();
        let session = current(&env);
        assert_eq!(
            env.review_recipient(session),
            None,
            "terminals are not chats"
        );
        let first = add_node(&mut env, NodeKind::Chat);
        let second = add_node(&mut env, NodeKind::Chat);
        assert_eq!(env.review_recipient(session), Some(first));
        env.remember_chat(second);
        assert_eq!(env.review_recipient(session), Some(second));
        if let Some(open) = env.current_session_mut() {
            let mut lead = SessionLead::new(AgentPreset::ClaudeCode, None);
            lead.terminal = Some(first);
            open.lead = Some(lead);
        }
        assert_eq!(env.review_recipient(session), Some(first));
    }

    #[test]
    fn a_workers_session_sends_its_review_to_the_worker() {
        let mut env = sessions_env();
        let lead_session = current(&env);
        let lead = add_node(&mut env, NodeKind::Chat);
        if let Some(open) = env.current_session_mut() {
            let mut owner = SessionLead::new(AgentPreset::ClaudeCode, None);
            owner.terminal = Some(lead);
            open.lead = Some(owner);
        }
        let worker = add_node(&mut env, NodeKind::Chat);
        let spawn = WorkerSpawn {
            lead: Some(lead_session),
            task: "webhook".to_owned(),
        };
        let Some(worker_session) = env.open_worker_session(worker, spawn) else {
            unreachable!("the worker's node exists");
        };
        assert_eq!(env.review_recipient(worker_session), Some(worker));
        assert_eq!(env.review_recipient(lead_session), Some(lead));
    }
}
