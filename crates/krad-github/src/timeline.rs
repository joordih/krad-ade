use krad_domain::review::{
    IssueComment, IssueEvent, IssueEventKind, LinkedRequest, MergeRequestIid, MergeRequestState,
    NoteId, TimelineEntry,
};
use serde::Deserialize;

use crate::issue::{IssueStateJson, LabelJson, ReactionsJson, TitledJson};
use crate::model::UserJson;

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
enum TimelineKind {
    Commented,
    Closed,
    Reopened,
    Labeled,
    Unlabeled,
    Assigned,
    Unassigned,
    Milestoned,
    Demilestoned,
    Renamed,
    CrossReferenced,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct RenameJson {
    from: String,
    to: String,
}

#[derive(Deserialize)]
struct MergedMarkerJson {
    #[serde(default)]
    merged_at: Option<String>,
}

#[derive(Deserialize)]
struct SourceIssueJson {
    number: u64,
    title: String,
    state: IssueStateJson,
    html_url: String,
    #[serde(default)]
    pull_request: Option<MergedMarkerJson>,
}

#[derive(Deserialize)]
struct SourceJson {
    #[serde(default)]
    issue: Option<SourceIssueJson>,
}

#[derive(Deserialize)]
struct NamedUserJson {
    login: String,
}

#[derive(Deserialize)]
pub(crate) struct TimelineJson {
    event: TimelineKind,
    #[serde(default)]
    id: u64,
    #[serde(default)]
    user: Option<UserJson>,
    #[serde(default)]
    actor: Option<UserJson>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    reactions: ReactionsJson,
    #[serde(default)]
    label: Option<LabelJson>,
    #[serde(default)]
    assignee: Option<NamedUserJson>,
    #[serde(default)]
    milestone: Option<TitledJson>,
    #[serde(default)]
    rename: Option<RenameJson>,
    #[serde(default)]
    source: Option<SourceJson>,
}

impl SourceIssueJson {
    fn into_linked(self) -> Option<LinkedRequest> {
        let pull = self.pull_request?;
        let state = match (pull.merged_at, self.state) {
            (Some(_), _) => MergeRequestState::Merged,
            (None, IssueStateJson::Open) => MergeRequestState::Opened,
            (None, IssueStateJson::Closed) => MergeRequestState::Closed,
        };
        Some(LinkedRequest {
            number: MergeRequestIid(self.number),
            title: self.title,
            state,
            web_url: self.html_url,
        })
    }
}

impl TimelineJson {
    fn event_kind(self) -> Option<IssueEventKind> {
        Some(match self.event {
            TimelineKind::Closed => IssueEventKind::Closed,
            TimelineKind::Reopened => IssueEventKind::Reopened,
            TimelineKind::Labeled => IssueEventKind::Labeled(self.label?.name),
            TimelineKind::Unlabeled => IssueEventKind::Unlabeled(self.label?.name),
            TimelineKind::Assigned => IssueEventKind::Assigned(self.assignee?.login),
            TimelineKind::Unassigned => IssueEventKind::Unassigned(self.assignee?.login),
            TimelineKind::Milestoned => IssueEventKind::Milestoned(self.milestone?.title),
            TimelineKind::Demilestoned => IssueEventKind::Demilestoned(self.milestone?.title),
            TimelineKind::Renamed => {
                let rename = self.rename?;
                IssueEventKind::Renamed {
                    from: rename.from,
                    to: rename.to,
                }
            }
            TimelineKind::CrossReferenced => {
                IssueEventKind::Referenced(self.source?.issue?.into_linked()?)
            }
            TimelineKind::Commented | TimelineKind::Other => return None,
        })
    }

    fn into_entry(mut self) -> Option<TimelineEntry> {
        if self.event == TimelineKind::Commented {
            return Some(TimelineEntry::Comment(IssueComment {
                id: NoteId(self.id),
                author: self.user?.into(),
                body: self.body.unwrap_or_default(),
                created_at: self.created_at,
                reactions: self.reactions.total_count,
            }));
        }
        let actor = self.actor.take().map(Into::into);
        let created_at = std::mem::take(&mut self.created_at);
        let kind = self.event_kind()?;
        Some(TimelineEntry::Event(IssueEvent {
            actor,
            kind,
            created_at,
        }))
    }
}

pub(crate) fn timeline_entries(listed: Vec<TimelineJson>) -> Vec<TimelineEntry> {
    listed
        .into_iter()
        .filter_map(TimelineJson::into_entry)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::review::IssueActivity;

    const TIMELINE: &str = include_str!("../fixtures/issue_timeline.json");

    #[test]
    fn the_timeline_keeps_comments_and_known_events_in_order() -> serde_json::Result<()> {
        let entries = timeline_entries(serde_json::from_str(TIMELINE)?);
        let kinds: Vec<String> = entries
            .iter()
            .map(|entry| match entry {
                TimelineEntry::Comment(comment) => {
                    format!(
                        "comment by {} +{}",
                        comment.author.username, comment.reactions
                    )
                }
                TimelineEntry::Event(event) => format!("{:?}", event.kind),
            })
            .collect();
        assert_eq!(
            kinds.len(),
            6,
            "subscriptions and plain issue mentions drop out"
        );
        assert_eq!(kinds[0], "Labeled(\"bug\")");
        assert_eq!(kinds[1], "comment by bo +2");
        assert_eq!(kinds[2], "Assigned(\"bo\")");
        assert!(kinds[3].starts_with("Referenced("));
        assert_eq!(
            kinds[4],
            "Renamed { from: \"Refunds broken\", to: \"Refund fails on Safari\" }"
        );
        assert_eq!(kinds[5], "Closed");
        Ok(())
    }

    #[test]
    fn a_merged_pull_request_that_mentions_the_issue_is_linked() -> serde_json::Result<()> {
        let activity = IssueActivity::new(
            timeline_entries(serde_json::from_str(TIMELINE)?),
            Vec::new(),
        );
        assert_eq!(activity.linked.len(), 1);
        let linked = &activity.linked[0];
        assert_eq!(linked.number, MergeRequestIid(51));
        assert_eq!(linked.state, MergeRequestState::Merged);
        assert_eq!(linked.web_url, "https://github.com/octo/shop/pull/51");
        Ok(())
    }
}
