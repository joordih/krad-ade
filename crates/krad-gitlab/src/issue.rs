use krad_domain::review::{
    Issue, IssueComment, IssueEvent, IssueEventKind, IssueNumber, IssueSort, IssueState,
    IssueStateFilter, LinkedRequest, MergeRequestIid, NoteId, TimelineEntry,
};
use serde::{Deserialize, Serialize};

use crate::model::{StateJson, UserJson};

const OPENED_STATE: &str = "opened";
const CLOSED_STATE: &str = "closed";
const ALL_STATE: &str = "all";
const CLOSE_EVENT: &str = "close";
const REOPEN_EVENT: &str = "reopen";
const CLOSED_NOTE: &str = "closed";
const REOPENED_NOTE: &str = "reopened";
const CREATED_ORDER: &str = "created_at";
const UPDATED_ORDER: &str = "updated_at";
const DESCENDING: &str = "desc";
const ASCENDING: &str = "asc";

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
enum IssueStateJson {
    Opened,
    Closed,
}

#[derive(Deserialize)]
struct MilestoneJson {
    title: String,
}

#[derive(Deserialize)]
pub(crate) struct IssueJson {
    iid: u64,
    title: String,
    #[serde(default)]
    description: Option<String>,
    web_url: String,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default = "opened")]
    state: IssueStateJson,
    #[serde(default)]
    author: Option<UserJson>,
    #[serde(default)]
    assignees: Vec<UserJson>,
    #[serde(default)]
    milestone: Option<MilestoneJson>,
    #[serde(default)]
    user_notes_count: u32,
    #[serde(default)]
    upvotes: u32,
    #[serde(default)]
    downvotes: u32,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
}

const fn opened() -> IssueStateJson {
    IssueStateJson::Opened
}

impl From<IssueJson> for Issue {
    fn from(issue: IssueJson) -> Self {
        Self {
            number: IssueNumber(issue.iid),
            title: issue.title,
            body: issue.description.unwrap_or_default(),
            web_url: issue.web_url,
            labels: issue.labels,
            state: match issue.state {
                IssueStateJson::Opened => IssueState::Open,
                IssueStateJson::Closed => IssueState::Closed,
            },
            author: issue.author.map(Into::into),
            assignees: issue.assignees.into_iter().map(Into::into).collect(),
            milestone: issue.milestone.map(|milestone| milestone.title),
            comments: issue.user_notes_count,
            reactions: issue.upvotes.saturating_add(issue.downvotes),
            created_at: issue.created_at,
            updated_at: issue.updated_at,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct IssueNoteJson {
    id: u64,
    body: String,
    author: UserJson,
    created_at: String,
    #[serde(default)]
    system: bool,
}

impl From<IssueNoteJson> for TimelineEntry {
    fn from(note: IssueNoteJson) -> Self {
        if !note.system {
            return Self::Comment(IssueComment {
                id: NoteId(note.id),
                author: note.author.into(),
                body: note.body,
                created_at: note.created_at,
                reactions: 0,
            });
        }
        let kind = match note.body.as_str() {
            CLOSED_NOTE => IssueEventKind::Closed,
            REOPENED_NOTE => IssueEventKind::Reopened,
            _ => IssueEventKind::Noted(note.body),
        };
        Self::Event(IssueEvent {
            actor: Some(note.author.into()),
            kind,
            created_at: note.created_at,
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct RelatedJson {
    iid: u64,
    title: String,
    state: StateJson,
    web_url: String,
}

impl From<RelatedJson> for LinkedRequest {
    fn from(related: RelatedJson) -> Self {
        Self {
            number: MergeRequestIid(related.iid),
            title: related.title,
            state: related.state.into(),
            web_url: related.web_url,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct StateEventBody {
    state_event: &'static str,
}

pub(crate) const fn state_event(state: IssueState) -> StateEventBody {
    StateEventBody {
        state_event: match state {
            IssueState::Open => REOPEN_EVENT,
            IssueState::Closed => CLOSE_EVENT,
        },
    }
}

pub(crate) const fn state_filter(state: IssueStateFilter) -> &'static str {
    match state {
        IssueStateFilter::Open => OPENED_STATE,
        IssueStateFilter::Closed => CLOSED_STATE,
        IssueStateFilter::All => ALL_STATE,
    }
}

pub(crate) const fn order(sort: IssueSort) -> (&'static str, &'static str) {
    match sort {
        IssueSort::Newest | IssueSort::MostCommented => (CREATED_ORDER, DESCENDING),
        IssueSort::Oldest => (CREATED_ORDER, ASCENDING),
        IssueSort::RecentlyUpdated => (UPDATED_ORDER, DESCENDING),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::review::{IssueActivity, MergeRequestState};

    const ISSUES: &str = include_str!("../fixtures/issues.json");
    const DETAILED: &str = include_str!("../fixtures/issue_detail.json");
    const NOTES: &str = include_str!("../fixtures/issue_notes.json");
    const RELATED: &str = include_str!("../fixtures/related_merge_requests.json");

    #[test]
    fn listed_issues_read_their_project_number_and_description() -> serde_json::Result<()> {
        let issues: Vec<Issue> = serde_json::from_str::<Vec<IssueJson>>(ISSUES)?
            .into_iter()
            .map(Issue::from)
            .collect();
        assert_eq!(
            issues[0].number,
            IssueNumber(7),
            "the iid, not the global id"
        );
        assert_eq!(issues[0].title, "Login token is not refreshed");
        assert_eq!(issues[0].body, "After an hour the app signs the user out.");
        assert_eq!(
            issues[0].web_url,
            "https://gitlab.example.com/team/app/-/issues/7"
        );
        assert_eq!(issues[0].labels, ["bug", "auth"]);
        assert_eq!(issues[0].state, IssueState::Open);
        assert_eq!(issues[1].body, "", "a null description reads as empty");
        Ok(())
    }

    #[test]
    fn an_issue_reads_its_people_milestone_and_votes() -> serde_json::Result<()> {
        let issue = Issue::from(serde_json::from_str::<IssueJson>(DETAILED)?);
        assert_eq!(issue.state, IssueState::Closed);
        assert_eq!(issue.assignees[0].username, "bo");
        assert_eq!(issue.milestone.as_deref(), Some("Sprint 14"));
        assert_eq!((issue.comments, issue.reactions), (4, 4));
        Ok(())
    }

    #[test]
    fn notes_become_comments_and_system_events() -> serde_json::Result<()> {
        let timeline: Vec<TimelineEntry> = serde_json::from_str::<Vec<IssueNoteJson>>(NOTES)?
            .into_iter()
            .map(TimelineEntry::from)
            .collect();
        let related: Vec<LinkedRequest> = serde_json::from_str::<Vec<RelatedJson>>(RELATED)?
            .into_iter()
            .map(LinkedRequest::from)
            .collect();
        let activity = IssueActivity::new(timeline, related);
        assert_eq!(activity.comment_count(), 1);
        let kinds: Vec<Option<&IssueEventKind>> = activity
            .timeline
            .iter()
            .map(|entry| match entry {
                TimelineEntry::Event(event) => Some(&event.kind),
                TimelineEntry::Comment(_) => None,
            })
            .collect();
        assert_eq!(
            kinds,
            [
                Some(&IssueEventKind::Noted("added ~bug label".to_owned())),
                None,
                Some(&IssueEventKind::Noted(
                    "mentioned in merge request !31".to_owned()
                )),
                Some(&IssueEventKind::Closed),
            ]
        );
        assert_eq!(activity.linked[0].number, MergeRequestIid(31));
        assert_eq!(activity.linked[0].state, MergeRequestState::Merged);
        Ok(())
    }

    #[test]
    fn states_and_orders_name_the_gitlab_values() {
        assert_eq!(state_filter(IssueStateFilter::Open), "opened");
        assert_eq!(state_event(IssueState::Closed).state_event, "close");
        assert_eq!(state_event(IssueState::Open).state_event, "reopen");
        assert_eq!(order(IssueSort::RecentlyUpdated), ("updated_at", "desc"));
        assert_eq!(order(IssueSort::Oldest), ("created_at", "asc"));
    }
}
